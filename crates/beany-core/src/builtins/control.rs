//! Linear system models (`tf`, `zpk`, `ss`, `frd`) and control analysis: model
//! algebra and interconnection, time and frequency responses, margins, root locus,
//! discretization and state-feedback design.
//!
//! Single-input single-output models are handled as rational functions; state-space
//! models keep their matrices so simulation and design work on any size.

use nalgebra::DMatrix;

use super::*;
use crate::ast::{BinOp, UnOp};
use crate::display::{self, Format};

pub const TF: &str = "tf";
pub const ZPK: &str = "zpk";
pub const SS: &str = "ss";
pub const FRD: &str = "frd";

type M = DMatrix<f64>;

fn c(re: f64, im: f64) -> C64 {
    C64::new(re, im)
}

// ---------------------------------------------------------------- models

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Tf,
    Zpk,
    Ss,
}

#[derive(Clone, Debug)]
pub struct Ss {
    a: M,
    b: M,
    c: M,
    d: M,
}

#[derive(Clone, Debug)]
struct Model {
    kind: Kind,
    /// Rational form (descending powers), for single-input single-output models.
    num: Vec<f64>,
    den: Vec<f64>,
    /// Zeros, poles and gain as given to zpk (kept for display).
    zpk: Option<(Vec<C64>, Vec<C64>, f64)>,
    ss: Option<Ss>,
    ts: f64,
    delay: f64,
}

impl Model {
    fn gain(k: f64) -> Model {
        Model {
            kind: Kind::Tf,
            num: vec![k],
            den: vec![1.0],
            zpk: None,
            ss: None,
            ts: 0.0,
            delay: 0.0,
        }
    }

    fn rational(num: Vec<f64>, den: Vec<f64>, ts: f64) -> Model {
        Model {
            kind: Kind::Tf,
            num: trim(num),
            den: trim(den),
            zpk: None,
            ss: None,
            ts,
            delay: 0.0,
        }
    }

    fn is_siso(&self) -> bool {
        self.ss
            .as_ref()
            .is_none_or(|s| s.b.ncols() == 1 && s.c.nrows() == 1)
    }

    /// Numerator and denominator (SISO only).
    fn rat(&self) -> MResult<(Vec<f64>, Vec<f64>)> {
        match &self.ss {
            None => Ok((self.num.clone(), self.den.clone())),
            Some(s) => {
                if !self.is_siso() {
                    return Err(merr!(
                        "This operation needs a single-input, single-output model."
                    ));
                }
                Ok(ss2tf(s))
            }
        }
    }

    fn state_space(&self) -> Ss {
        match &self.ss {
            Some(s) => s.clone(),
            None => tf2ss(&self.num, &self.den),
        }
    }

    fn poles(&self) -> Vec<C64> {
        match &self.ss {
            Some(s) => eigenvalues(&s.a),
            None => match &self.zpk {
                Some((_, p, _)) => p.clone(),
                None => roots(&self.den),
            },
        }
    }

    fn zeros(&self) -> MResult<Vec<C64>> {
        if let Some((z, _, _)) = &self.zpk {
            return Ok(z.clone());
        }
        let (n, _) = self.rat()?;
        Ok(roots(&n))
    }

    /// Response at the complex frequency `s` (or `z` for discrete models), SISO.
    fn eval(&self, s: C64) -> MResult<C64> {
        let (n, d) = self.rat()?;
        let v = peval(&n, s) / peval(&d, s);
        Ok(if self.delay > 0.0 && self.ts == 0.0 {
            v * (-s * self.delay).exp()
        } else if self.delay > 0.0 {
            v * s.powf(-self.delay)
        } else {
            v
        })
    }

    fn freq(&self, w: f64) -> MResult<C64> {
        if self.ts > 0.0 {
            self.eval(c(0.0, w * self.ts).exp())
        } else if self.ts < 0.0 {
            self.eval(c(0.0, w).exp())
        } else {
            self.eval(c(0.0, w))
        }
    }

    fn with_kind(mut self, k: Kind) -> Model {
        self.kind = k;
        if k != Kind::Zpk {
            self.zpk = None;
        }
        if k != Kind::Ss {
            if let Some(s) = self.ss.take() {
                let (n, d) = ss2tf(&s);
                self.num = n;
                self.den = d;
            }
        } else if self.ss.is_none() {
            self.ss = Some(tf2ss(&self.num, &self.den));
        }
        self
    }
}

fn obj_model(v: &Value) -> MResult<Model> {
    match v {
        Value::Obj(o) if matches!(o.class, TF | ZPK | SS) => {
            let ts = o.get("Ts").map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
            let delay = o
                .get("InputDelay")
                .map(|v| v.scalar())
                .transpose()?
                .unwrap_or(0.0)
                + o.get("IODelay")
                    .map(|v| v.scalar())
                    .transpose()?
                    .unwrap_or(0.0);
            match o.class {
                TF => {
                    let first = |k: &str| -> MResult<Vec<f64>> {
                        match o.get(k) {
                            Some(Value::Cell(cl)) => Ok(cl.data[0].to_num()?.data),
                            Some(v) => Ok(v.to_num()?.data),
                            None => Ok(vec![1.0]),
                        }
                    };
                    Ok(Model {
                        kind: Kind::Tf,
                        num: trim(first("Numerator")?),
                        den: trim(first("Denominator")?),
                        zpk: None,
                        ss: None,
                        ts,
                        delay,
                    })
                }
                ZPK => {
                    let get = |k: &str| -> MResult<Vec<C64>> {
                        match o.get(k) {
                            Some(Value::Cell(cl)) => Ok(cl.data[0].to_cplx()?.data),
                            Some(v) => Ok(v.to_cplx()?.data),
                            None => Ok(vec![]),
                        }
                    };
                    let z = get("Z")?;
                    let p = get("P")?;
                    let k = o.get("K").map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
                    let num: Vec<f64> = poly_real(&z).iter().map(|x| x * k).collect();
                    let den = poly_real(&p);
                    Ok(Model {
                        kind: Kind::Zpk,
                        num: trim(num),
                        den: trim(den),
                        zpk: Some((z, p, k)),
                        ss: None,
                        ts,
                        delay,
                    })
                }
                _ => {
                    let m = |k: &str| -> MResult<M> {
                        let a = o
                            .get(k)
                            .map(|v| v.to_num())
                            .transpose()?
                            .unwrap_or_else(Array::empty);
                        Ok(crate::ops::to_dmatrix(&a))
                    };
                    let (a, b, cm, d) = (m("A")?, m("B")?, m("C")?, m("D")?);
                    Ok(Model {
                        kind: Kind::Ss,
                        num: vec![],
                        den: vec![],
                        zpk: None,
                        ss: Some(Ss { a, b, c: cm, d }),
                        ts,
                        delay,
                    })
                }
            }
        }
        Value::Obj(o) => Err(merr!("Expected a linear system model, got {}.", o.class)),
        v => {
            let g = v.to_num()?;
            if g.numel() == 1 {
                Ok(Model::gain(g.data[0]))
            } else {
                let d = crate::ops::to_dmatrix(&g);
                let ss = Ss {
                    a: M::zeros(0, 0),
                    b: M::zeros(0, d.ncols()),
                    c: M::zeros(d.nrows(), 0),
                    d,
                };
                Ok(Model {
                    kind: Kind::Ss,
                    num: vec![],
                    den: vec![],
                    zpk: None,
                    ss: Some(ss),
                    ts: 0.0,
                    delay: 0.0,
                })
            }
        }
    }
}

pub fn is_lti(v: &Value) -> bool {
    matches!(v, Value::Obj(o) if matches!(o.class, TF | ZPK | SS | FRD))
}

fn mval(m: &M) -> Value {
    Value::mat(crate::ops::from_dmatrix(m))
}

fn model_value(m: Model) -> Value {
    let mut props: Vec<(String, Value)> = match m.kind {
        Kind::Tf => {
            let (n, d) = m.rat().unwrap_or((vec![0.0], vec![1.0]));
            vec![
                ("Numerator".into(), Value::cell_row(vec![Value::row(n)])),
                ("Denominator".into(), Value::cell_row(vec![Value::row(d)])),
                (
                    "Variable".into(),
                    Value::chars(if m.ts != 0.0 { "z" } else { "s" }),
                ),
                ("IODelay".into(), Value::num(0.0)),
            ]
        }
        Kind::Zpk => {
            let (z, p, k) = match &m.zpk {
                Some(t) => t.clone(),
                None => {
                    let (n, d) = m.rat().unwrap_or((vec![0.0], vec![1.0]));
                    let lead = |v: &[f64]| v.first().copied().unwrap_or(1.0);
                    (clean(roots(&n)), clean(roots(&d)), lead(&n) / lead(&d))
                }
            };
            vec![
                (
                    "Z".into(),
                    Value::cell_row(vec![Value::complex(Array::col(z))]),
                ),
                (
                    "P".into(),
                    Value::cell_row(vec![Value::complex(Array::col(p))]),
                ),
                ("K".into(), Value::num(k)),
                (
                    "Variable".into(),
                    Value::chars(if m.ts != 0.0 { "z" } else { "s" }),
                ),
                ("IODelay".into(), Value::num(0.0)),
            ]
        }
        Kind::Ss => {
            let s = m.state_space();
            vec![
                ("A".into(), mval(&s.a)),
                ("B".into(), mval(&s.b)),
                ("C".into(), mval(&s.c)),
                ("D".into(), mval(&s.d)),
                ("E".into(), Value::empty()),
                (
                    "StateName".into(),
                    Value::Cell(Array::new(
                        s.a.nrows(),
                        1,
                        vec![Value::chars(""); s.a.nrows()],
                    )),
                ),
            ]
        }
    };
    props.push(("InputDelay".into(), Value::num(m.delay)));
    props.push(("OutputDelay".into(), Value::num(0.0)));
    props.push(("Ts".into(), Value::num(m.ts)));
    props.push(("TimeUnit".into(), Value::chars("seconds")));
    props.push(("InputName".into(), Value::cell_row(vec![Value::chars("")])));
    props.push(("OutputName".into(), Value::cell_row(vec![Value::chars("")])));
    props.push(("Name".into(), Value::chars("")));
    props.push(("Notes".into(), Value::Cell(Array::new(0, 1, vec![]))));
    props.push(("UserData".into(), Value::empty()));
    let class = match m.kind {
        Kind::Tf => TF,
        Kind::Zpk => ZPK,
        Kind::Ss => SS,
    };
    Value::Obj(Box::new(Object::new(class, props)))
}

// ---------------------------------------------------------------- polynomials

fn trim(mut v: Vec<f64>) -> Vec<f64> {
    while v.len() > 1 && v[0] == 0.0 {
        v.remove(0);
    }
    if v.is_empty() {
        v.push(0.0);
    }
    v
}

fn conv(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return vec![0.0];
    }
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

fn padd(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len().max(b.len());
    let mut out = vec![0.0; n];
    for (i, x) in a.iter().enumerate() {
        out[n - a.len() + i] += x;
    }
    for (i, x) in b.iter().enumerate() {
        out[n - b.len() + i] += x;
    }
    trim(out)
}

fn pscale(a: &[f64], k: f64) -> Vec<f64> {
    a.iter().map(|x| x * k).collect()
}

fn peval(p: &[f64], s: C64) -> C64 {
    p.iter().fold(c(0.0, 0.0), |acc, k| acc * s + k)
}

fn roots(p: &[f64]) -> Vec<C64> {
    let mut p = trim(p.to_vec());
    let mut zeros = 0;
    while p.len() > 1 && *p.last().unwrap() == 0.0 {
        p.pop();
        zeros += 1;
    }
    let n = p.len() - 1;
    let mut out = Vec::new();
    if n >= 1 && p[0] != 0.0 {
        let mut m = M::zeros(n, n);
        for j in 0..n {
            m[(0, j)] = -p[j + 1] / p[0];
        }
        for i in 1..n {
            m[(i, i - 1)] = 1.0;
        }
        out = clean(m.complex_eigenvalues().iter().copied().collect());
    }
    out.extend(std::iter::repeat_n(c(0.0, 0.0), zeros));
    out
}

/// Snap rounding-level imaginary parts to zero.
fn clean(v: Vec<C64>) -> Vec<C64> {
    v.into_iter()
        .map(|z| {
            if z.im.abs() <= 1e-10 * z.norm().max(1e-300) {
                c(z.re, 0.0)
            } else {
                z
            }
        })
        .collect()
}

/// Real polynomial with the given roots (conjugate pairs give real coefficients).
fn poly_real(r: &[C64]) -> Vec<f64> {
    let mut p = vec![c(1.0, 0.0)];
    for x in r {
        let mut q = vec![c(0.0, 0.0); p.len() + 1];
        for (i, k) in p.iter().enumerate() {
            q[i] += k;
            q[i + 1] -= k * x;
        }
        p = q;
    }
    p.iter().map(|z| z.re).collect()
}

fn eigenvalues(a: &M) -> Vec<C64> {
    if a.nrows() == 0 {
        return vec![];
    }
    clean(a.clone().complex_eigenvalues().iter().copied().collect())
}

/// Characteristic polynomial det(sI - A).
fn charpoly(a: &M) -> Vec<f64> {
    poly_real(&eigenvalues(a))
}

// ---------------------------------------------------------------- conversions

/// Controllable canonical form, as tf2ss returns it.
fn tf2ss(num: &[f64], den: &[f64]) -> Ss {
    let den = trim(den.to_vec());
    let n = den.len() - 1;
    let a0 = den[0];
    let a: Vec<f64> = den.iter().map(|x| x / a0).collect();
    let mut b = vec![0.0; (n + 1).saturating_sub(num.len())];
    b.extend(num.iter().map(|x| x / a0));
    let b = b[b.len() - (n + 1)..].to_vec();
    let mut am = M::zeros(n, n);
    for j in 0..n {
        am[(0, j)] = -a[j + 1];
    }
    for i in 1..n {
        am[(i, i - 1)] = 1.0;
    }
    let mut bm = M::zeros(n, 1);
    if n > 0 {
        bm[(0, 0)] = 1.0;
    }
    let mut cm = M::zeros(1, n);
    for j in 0..n {
        cm[(0, j)] = b[j + 1] - a[j + 1] * b[0];
    }
    Ss {
        a: am,
        b: bm,
        c: cm,
        d: M::from_element(1, 1, b[0]),
    }
}

/// SISO transfer function of a state-space model:
/// C(sI-A)^-1 B = (det(sI-A+BC) - det(sI-A)) / det(sI-A).
fn ss2tf(s: &Ss) -> (Vec<f64>, Vec<f64>) {
    let n = s.a.nrows();
    let d = if s.d.nrows() > 0 && s.d.ncols() > 0 {
        s.d[(0, 0)]
    } else {
        0.0
    };
    if n == 0 {
        return (vec![d], vec![1.0]);
    }
    let den = charpoly(&s.a);
    let bc = s.b.columns(0, 1) * s.c.rows(0, 1);
    let shifted = charpoly(&(&s.a - bc));
    let mut num = padd(&shifted, &pscale(&den, -1.0));
    num = padd(&num, &pscale(&den, d));
    // Rounding leaves tiny leading terms where the true coefficient is zero.
    let scale = num.iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-300);
    let num: Vec<f64> = num
        .iter()
        .map(|x| if x.abs() < 1e-12 * scale { 0.0 } else { *x })
        .collect();
    (trim(num), den)
}

// ---------------------------------------------------------------- algebra

fn result_kind(a: &Model, b: &Model) -> Kind {
    match (a.kind, b.kind) {
        (Kind::Ss, _) | (_, Kind::Ss) => Kind::Ss,
        (Kind::Zpk, _) | (_, Kind::Zpk) => Kind::Zpk,
        _ => Kind::Tf,
    }
}

fn common_ts(a: &Model, b: &Model) -> MResult<f64> {
    let static_a = a.ss.is_none() && a.den.len() == 1 && a.num.len() == 1;
    let static_b = b.ss.is_none() && b.den.len() == 1 && b.num.len() == 1;
    if a.ts == b.ts || static_b {
        Ok(a.ts)
    } else if static_a {
        Ok(b.ts)
    } else {
        Err(merr!("Sampling times must agree."))
    }
}

fn ss_series(s1: &Ss, s2: &Ss) -> Ss {
    // u -> s1 -> s2 -> y
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let mut a = M::zeros(n1 + n2, n1 + n2);
    a.view_mut((0, 0), (n1, n1)).copy_from(&s1.a);
    a.view_mut((n1, 0), (n2, n1)).copy_from(&(&s2.b * &s1.c));
    a.view_mut((n1, n1), (n2, n2)).copy_from(&s2.a);
    let mut b = M::zeros(n1 + n2, s1.b.ncols());
    b.view_mut((0, 0), (n1, s1.b.ncols())).copy_from(&s1.b);
    b.view_mut((n1, 0), (n2, s1.b.ncols()))
        .copy_from(&(&s2.b * &s1.d));
    let mut cm = M::zeros(s2.c.nrows(), n1 + n2);
    cm.view_mut((0, 0), (s2.c.nrows(), n1))
        .copy_from(&(&s2.d * &s1.c));
    cm.view_mut((0, n1), (s2.c.nrows(), n2)).copy_from(&s2.c);
    Ss {
        a,
        b,
        c: cm,
        d: &s2.d * &s1.d,
    }
}

fn ss_parallel(s1: &Ss, s2: &Ss, sign: f64) -> Ss {
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let mut a = M::zeros(n1 + n2, n1 + n2);
    a.view_mut((0, 0), (n1, n1)).copy_from(&s1.a);
    a.view_mut((n1, n1), (n2, n2)).copy_from(&s2.a);
    let m = s1.b.ncols();
    let mut b = M::zeros(n1 + n2, m);
    b.view_mut((0, 0), (n1, m)).copy_from(&s1.b);
    b.view_mut((n1, 0), (n2, m)).copy_from(&s2.b);
    let p = s1.c.nrows();
    let mut cm = M::zeros(p, n1 + n2);
    cm.view_mut((0, 0), (p, n1)).copy_from(&s1.c);
    cm.view_mut((0, n1), (p, n2)).copy_from(&(&s2.c * sign));
    Ss {
        a,
        b,
        c: cm,
        d: &s1.d + &s2.d * sign,
    }
}

/// Closed loop of `s1` with `s2` in the feedback path (`sign` -1 for negative feedback).
fn ss_feedback(s1: &Ss, s2: &Ss, sign: f64) -> MResult<Ss> {
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let p = s1.c.nrows();
    let m = s1.b.ncols();
    // e = u + sign*y2, y2 = C2 x2 + D2 y; y = C1 x1 + D1 e.
    let i_p = M::identity(p, p);
    let e = (&i_p - &s1.d * &s2.d * sign)
        .try_inverse()
        .ok_or_else(|| merr!("The feedback loop is algebraically singular."))?;
    // y = E (C1 x1 + sign D1 C2 x2 + D1 u)
    let cy1 = &e * &s1.c;
    let cy2 = &e * &s1.d * &s2.c * sign;
    let dy = &e * &s1.d;
    // e_in = u + sign (C2 x2 + D2 y)
    let ce1 = &s2.d * &cy1 * sign;
    let ce2 = (&s2.c + &s2.d * &cy2) * sign;
    let de = M::identity(m, m) + &s2.d * &dy * sign;
    let mut a = M::zeros(n1 + n2, n1 + n2);
    a.view_mut((0, 0), (n1, n1))
        .copy_from(&(&s1.a + &s1.b * &ce1));
    a.view_mut((0, n1), (n1, n2)).copy_from(&(&s1.b * &ce2));
    a.view_mut((n1, 0), (n2, n1)).copy_from(&(&s2.b * &cy1));
    a.view_mut((n1, n1), (n2, n2))
        .copy_from(&(&s2.a + &s2.b * &cy2));
    let mut b = M::zeros(n1 + n2, m);
    b.view_mut((0, 0), (n1, m)).copy_from(&(&s1.b * &de));
    b.view_mut((n1, 0), (n2, m)).copy_from(&(&s2.b * &dy));
    let mut cm = M::zeros(p, n1 + n2);
    cm.view_mut((0, 0), (p, n1)).copy_from(&cy1);
    cm.view_mut((0, n1), (p, n2)).copy_from(&cy2);
    Ok(Ss { a, b, c: cm, d: dy })
}

fn mul(a: &Model, b: &Model) -> MResult<Model> {
    let ts = common_ts(a, b)?;
    let kind = result_kind(a, b);
    let mut m = if kind == Kind::Ss {
        // a*b means the signal passes through b first.
        let s = ss_series(&b.state_space(), &a.state_space());
        Model {
            kind,
            num: vec![],
            den: vec![],
            zpk: None,
            ss: Some(s),
            ts,
            delay: 0.0,
        }
    } else {
        let (n1, d1) = a.rat()?;
        let (n2, d2) = b.rat()?;
        let mut m = Model::rational(conv(&n1, &n2), conv(&d1, &d2), ts);
        if let (Some((z1, p1, k1)), Some((z2, p2, k2))) = (zpk_of(a), zpk_of(b)) {
            m.zpk = Some(([z1, z2].concat(), [p1, p2].concat(), k1 * k2));
        }
        m.kind = kind;
        m
    };
    m.ts = ts;
    m.delay = a.delay + b.delay;
    Ok(m)
}

fn zpk_of(m: &Model) -> Option<(Vec<C64>, Vec<C64>, f64)> {
    if let Some(t) = &m.zpk {
        return Some(t.clone());
    }
    let (n, d) = m.rat().ok()?;
    let lead = |v: &[f64]| v.first().copied().unwrap_or(1.0);
    Some((roots(&n), roots(&d), lead(&n) / lead(&d)))
}

fn add(a: &Model, b: &Model, sign: f64) -> MResult<Model> {
    let ts = common_ts(a, b)?;
    let kind = result_kind(a, b);
    if a.delay != b.delay {
        return Err(merr!(
            "Models with different time delays cannot be added exactly."
        ));
    }
    let mut m = if kind == Kind::Ss {
        Model {
            kind,
            num: vec![],
            den: vec![],
            zpk: None,
            ss: Some(ss_parallel(&a.state_space(), &b.state_space(), sign)),
            ts,
            delay: 0.0,
        }
    } else {
        let (n1, d1) = a.rat()?;
        let (n2, d2) = b.rat()?;
        let (num, den) = if d1 == d2 {
            (padd(&n1, &pscale(&n2, sign)), d1)
        } else {
            (
                padd(&conv(&n1, &d2), &pscale(&conv(&n2, &d1), sign)),
                conv(&d1, &d2),
            )
        };
        let mut m = Model::rational(num, den, ts);
        m.kind = kind;
        m
    };
    m.delay = a.delay;
    m.ts = ts;
    Ok(m)
}

fn inv(a: &Model) -> MResult<Model> {
    if a.kind == Kind::Ss {
        let s = a.state_space();
        let dinv =
            s.d.clone()
                .try_inverse()
                .ok_or_else(|| merr!("Cannot invert a model whose D matrix is singular."))?;
        let ss = Ss {
            a: &s.a - &s.b * &dinv * &s.c,
            b: &s.b * &dinv,
            c: -(&dinv * &s.c),
            d: dinv,
        };
        return Ok(Model {
            ss: Some(ss),
            ..a.clone()
        });
    }
    let (n, d) = a.rat()?;
    if n.iter().all(|x| *x == 0.0) {
        return Err(merr!("Cannot invert a model that is zero."));
    }
    let mut m = Model::rational(d, n, a.ts);
    m.kind = a.kind;
    m.zpk = a
        .zpk
        .as_ref()
        .map(|(z, p, k)| (p.clone(), z.clone(), 1.0 / k));
    Ok(m)
}

fn power(a: &Model, p: f64) -> MResult<Model> {
    if p.fract() != 0.0 {
        return Err(merr!("Model powers must be integers."));
    }
    let base = if p < 0.0 { inv(a)? } else { a.clone() };
    let mut r = Model::gain(1.0);
    r.ts = a.ts;
    r.kind = a.kind;
    for _ in 0..(p.abs() as usize) {
        r = mul(&r, &base)?;
    }
    if p == 0.0 && a.kind == Kind::Ss {
        r = r.with_kind(Kind::Ss);
    }
    Ok(r)
}

/// Operators on models (called by the interpreter for `tf`/`zpk`/`ss` operands).
pub fn lti_binary(op: BinOp, l: &Value, r: &Value) -> MResult<Value> {
    let a = obj_model(l)?;
    let b = obj_model(r)?;
    let m = match op {
        BinOp::Add => add(&a, &b, 1.0)?,
        BinOp::Sub => add(&a, &b, -1.0)?,
        BinOp::MatMul | BinOp::ElMul => mul(&a, &b)?,
        BinOp::MatDiv | BinOp::ElDiv => mul(&a, &inv(&b)?)?,
        BinOp::MatLDiv | BinOp::ElLDiv => mul(&inv(&a)?, &b)?,
        BinOp::MatPow | BinOp::ElPow => {
            let p = r.scalar()?;
            power(&a, p)?
        }
        BinOp::Eq | BinOp::Ne => {
            let same = a.rat()? == b.rat()? && a.ts == b.ts;
            return Ok(bool_val(same == (op == BinOp::Eq)));
        }
        _ => {
            return Err(merr!(
                "Operator '{}' is not supported for linear system models.",
                crate::ops::op_symbol(op)
            ))
        }
    };
    Ok(model_value(m))
}

pub fn lti_unary(op: UnOp, v: &Value) -> MResult<Value> {
    let a = obj_model(v)?;
    match op {
        UnOp::Neg => Ok(model_value(mul(&Model::gain(-1.0), &a)?.with_kind(a.kind))),
        UnOp::Plus => Ok(v.clone()),
        UnOp::Not => Err(merr!(
            "Operator '~' is not supported for linear system models."
        )),
    }
}

// ---------------------------------------------------------------- display

fn poly_text(p: &[f64], var: &str) -> String {
    let n = p.len();
    let mut s = String::new();
    for (i, &k) in p.iter().enumerate() {
        if k == 0.0 {
            continue;
        }
        let pow = n - 1 - i;
        let mag = k.abs();
        let coef = if (mag - 1.0).abs() < 1e-12 && pow > 0 {
            String::new()
        } else {
            display::trim_g(mag, 4)
        };
        let sep = if coef.is_empty() { "" } else { " " };
        let term = match pow {
            0 => coef,
            1 => format!("{}{}{}", coef, sep, var),
            _ => format!("{}{}{}^{}", coef, sep, var, pow),
        };
        if s.is_empty() {
            s = if k < 0.0 { format!("-{}", term) } else { term };
        } else {
            s.push_str(if k < 0.0 { " - " } else { " + " });
            s.push_str(&term);
        }
    }
    if s.is_empty() {
        "0".into()
    } else {
        s
    }
}

fn fraction(n: &str, d: &str) -> String {
    if d == "1" {
        return format!("  {}\n \n", n);
    }
    let w = n.chars().count().max(d.chars().count());
    let center = |t: &str| {
        let pad = (w - t.chars().count()) / 2;
        format!("{}{}", " ".repeat(pad), t)
    };
    format!("  {}\n  {}\n  {}\n \n", center(n), "-".repeat(w), center(d))
}

/// `(s+1) (s^2 + 2s + 5)` factors for zpk display.
fn factors(r: &[C64], var: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut used = vec![false; r.len()];
    for i in 0..r.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let z = r[i];
        if z.im.abs() > 0.0 {
            if let Some(j) = (i + 1..r.len())
                .find(|&j| !used[j] && (r[j] - z.conj()).norm() < 1e-9 * z.norm().max(1.0))
            {
                used[j] = true;
                let b = -2.0 * z.re;
                let k = z.norm_sqr();
                let mid = if b.abs() < 1e-14 {
                    String::new()
                } else {
                    format!(
                        " {} {}{}",
                        if b < 0.0 { "-" } else { "+" },
                        display::trim_g(b.abs(), 4),
                        var
                    )
                };
                parts.push(format!("({}^2{} + {})", var, mid, display::trim_g(k, 4)));
                continue;
            }
        }
        if z.re == 0.0 && z.im == 0.0 {
            parts.push(var.to_string());
        } else {
            parts.push(format!(
                "({}{}{})",
                var,
                if z.re < 0.0 { "+" } else { "-" },
                display::trim_g(z.re.abs(), 4)
            ));
        }
    }
    // Repeated factors: s^2, (s+1)^2.
    let mut out: Vec<String> = Vec::new();
    let mut k = 0;
    while k < parts.len() {
        let mut n = 1;
        while k + n < parts.len() && parts[k + n] == parts[k] {
            n += 1;
        }
        out.push(if n > 1 {
            format!("{}^{}", parts[k], n)
        } else {
            parts[k].clone()
        });
        k += n;
    }
    out.join(" ")
}

fn kind_footer(ts: f64, what: &str) -> String {
    if ts > 0.0 {
        format!(
            "Sample time: {} seconds\nDiscrete-time {}.\n",
            display::trim_g(ts, 6),
            what
        )
    } else if ts < 0.0 {
        format!("Sample time: unspecified\nDiscrete-time {}.\n", what)
    } else {
        format!("Continuous-time {}.\n", what)
    }
}

fn matrix_block(name: &str, m: &M, rows: &str, cols: &str) -> String {
    let mut s = format!("  {} = \n", name);
    if m.nrows() == 0 || m.ncols() == 0 {
        return s + "    []\n \n";
    }
    let cells: Vec<String> = m.iter().map(|x| display::trim_g(*x, 4)).collect();
    let w = cells
        .iter()
        .map(|t| t.len())
        .chain((1..=m.ncols()).map(|j| format!("{}{}", cols, j).len()))
        .max()
        .unwrap_or(1)
        + 3;
    let lw = format!("{}{}", rows, m.nrows()).len() + 3;
    s.push_str(&" ".repeat(lw));
    for j in 0..m.ncols() {
        s.push_str(&format!("{:>w$}", format!("{}{}", cols, j + 1), w = w));
    }
    s.push('\n');
    for i in 0..m.nrows() {
        s.push_str(&format!("{:>lw$}", format!("{}{}", rows, i + 1), lw = lw));
        for j in 0..m.ncols() {
            s.push_str(&format!("{:>w$}", cells[j * m.nrows() + i], w = w));
        }
        s.push('\n');
    }
    s.push_str(" \n");
    s
}

/// Command-window text for an LTI object.
pub fn lti_text(o: &Object, _fmt: Format) -> String {
    let Ok(m) = obj_model(&Value::Obj(Box::new(o.clone()))) else {
        return String::new();
    };
    let var = if m.ts != 0.0 { "z" } else { "s" };
    let delay = if m.delay > 0.0 {
        if m.ts == 0.0 {
            format!("  Input delay: {} seconds\n", display::trim_g(m.delay, 4))
        } else {
            format!("  Input delay: {} samples\n", display::trim_g(m.delay, 4))
        }
    } else {
        String::new()
    };
    match m.kind {
        Kind::Tf => {
            let (n, d) = m.rat().unwrap_or((vec![0.0], vec![1.0]));
            let mut s = format!(" \n{}", fraction(&poly_text(&n, var), &poly_text(&d, var)));
            if m.delay > 0.0 && m.ts == 0.0 {
                s = format!(
                    " \n{}",
                    fraction(
                        &format!(
                            "exp(-{}*s) * ({})",
                            display::trim_g(m.delay, 4),
                            poly_text(&n, var)
                        ),
                        &poly_text(&d, var),
                    )
                );
            }
            s + &kind_footer(m.ts, "transfer function")
        }
        Kind::Zpk => {
            let (z, p, k) = zpk_of(&m).unwrap_or_default();
            let zs = factors(&z, var);
            let ps = factors(&p, var);
            let ks = display::trim_g(k, 5);
            let num = match (zs.is_empty(), ks.as_str()) {
                (true, _) => ks.clone(),
                (false, "1") => zs,
                (false, "-1") => format!("-{}", zs),
                (false, _) => format!("{} {}", ks, zs),
            };
            let den = if ps.is_empty() { "1".to_string() } else { ps };
            format!(" \n{}{}", fraction(&num, &den), delay)
                + &kind_footer(m.ts, "zero/pole/gain model")
        }
        Kind::Ss => {
            let s = m.state_space();
            let mut t = String::from(" \n");
            t.push_str(&matrix_block("A", &s.a, "x", "x"));
            t.push_str(&matrix_block("B", &s.b, "x", "u"));
            t.push_str(&matrix_block("C", &s.c, "y", "x"));
            t.push_str(&matrix_block("D", &s.d, "y", "u"));
            t.push_str(&delay);
            t + &kind_footer(m.ts, "state-space model")
        }
    }
}

pub fn frd_text(o: &Object) -> String {
    let r = o
        .get("ResponseData")
        .and_then(|v| v.to_cplx().ok())
        .unwrap_or_else(Array::empty);
    let w = o
        .get("Frequency")
        .and_then(|v| v.to_num().ok())
        .unwrap_or_else(Array::empty);
    let mut s = String::from(
        "  Frequency(rad/s)          Response\n  ----------------          --------\n",
    );
    for (f, z) in w.data.iter().zip(&r.data) {
        s.push_str(&format!(
            "  {:>16}          {}\n",
            display::trim_g(*f, 5),
            display::cplx_to_str(*z)
        ));
    }
    s + " \nContinuous-time frequency response.\n"
}

// ---------------------------------------------------------------- constructors

fn tf_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return one(model_value(Model::gain(0.0)));
    }
    if a.len() == 1 && a[0].is_text() {
        let v = a[0].text()?;
        let m = match v.as_str() {
            "s" | "p" => Model::rational(vec![1.0, 0.0], vec![1.0], 0.0),
            "z" | "q" => Model::rational(vec![1.0, 0.0], vec![1.0], -1.0),
            _ => return Err(merr!("The variable must be 's', 'p', 'z' or 'q'.")),
        };
        return one(model_value(m));
    }
    if a.len() >= 2 && a[1].is_text() && a[0].is_text() {
        // tf('z', Ts)
        return Err(merr!("Invalid tf syntax."));
    }
    if a.len() == 2 && a[0].is_text() {
        let ts = a[1].scalar()?;
        return one(model_value(Model::rational(vec![1.0, 0.0], vec![1.0], ts)));
    }
    if a.len() == 1 {
        // Conversion, or a static gain.
        let m = obj_model(&a[0])?;
        return one(model_value(m.with_kind(Kind::Tf)));
    }
    let cell_or_vec = |v: &Value| -> MResult<Vec<f64>> {
        match v {
            Value::Cell(cl) => Ok(cl.data[0].to_num()?.data),
            v => Ok(v.to_num()?.data),
        }
    };
    let num = cell_or_vec(&a[0])?;
    let den = cell_or_vec(&a[1])?;
    if den.iter().all(|x| *x == 0.0) {
        return Err(merr!("The denominator must not be zero."));
    }
    let mut m = Model::rational(num, den, 0.0);
    let mut i = 2;
    if a.len() > 2 && !a[2].is_text() {
        m.ts = a[2].scalar()?;
        if m.ts < 0.0 && m.ts != -1.0 {
            return Err(merr!(
                "The sample time must be a positive scalar, -1, or 0."
            ));
        }
        i = 3;
    }
    apply_props(&mut m, &a[i..])?;
    one(model_value(m))
}

/// Name/value options shared by the model constructors.
fn apply_props(m: &mut Model, rest: &[Value]) -> MResult<()> {
    let mut k = 0;
    while k + 1 < rest.len() {
        let name = rest[k].text()?;
        match name.to_lowercase().as_str() {
            "inputdelay" | "iodelay" | "outputdelay" => m.delay += rest[k + 1].scalar()?,
            "ts" => m.ts = rest[k + 1].scalar()?,
            "variable"
                if matches!(rest[k + 1].text()?.as_str(), "z" | "q" | "z^-1") && m.ts == 0.0 =>
            {
                m.ts = -1.0
            }
            _ => {}
        }
        k += 2;
    }
    Ok(())
}

fn zpk_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() == 1 {
        if a[0].is_text() {
            let m = Model::rational(
                vec![1.0, 0.0],
                vec![1.0],
                if a[0].text()? == "z" { -1.0 } else { 0.0 },
            );
            return one(model_value(m.with_kind(Kind::Zpk)));
        }
        let m = obj_model(&a[0])?;
        return one(model_value(m.with_kind(Kind::Zpk)));
    }
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let cv = |v: &Value| -> MResult<Vec<C64>> {
        match v {
            Value::Cell(cl) => Ok(cl.data[0].to_cplx()?.data),
            v => Ok(v.to_cplx()?.data),
        }
    };
    let z = cv(&a[0])?;
    let p = cv(&a[1])?;
    let k = a[2].scalar()?;
    let mut m = Model::rational(pscale(&poly_real(&z), k), poly_real(&p), 0.0);
    m.kind = Kind::Zpk;
    m.zpk = Some((z, p, k));
    let mut i = 3;
    if a.len() > 3 && !a[3].is_text() {
        m.ts = a[3].scalar()?;
        i = 4;
    }
    apply_props(&mut m, &a[i..])?;
    one(model_value(m))
}

fn ss_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() == 1 {
        let m = obj_model(&a[0])?;
        return one(model_value(m.with_kind(Kind::Ss)));
    }
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let mat = |v: &Value| -> MResult<M> { Ok(crate::ops::to_dmatrix(&v.to_num()?)) };
    let am = mat(&a[0])?;
    let n = am.nrows();
    if am.ncols() != n {
        return Err(merr!("The A matrix must be square."));
    }
    let mut bm = mat(&a[1])?;
    let mut cm = mat(&a[2])?;
    let mut dm = if a.len() > 3 {
        mat(&a[3])?
    } else {
        M::zeros(0, 0)
    };
    if bm.nrows() == 0 && n == 0 {
        bm = M::zeros(0, dm.ncols());
    }
    if cm.ncols() == 0 && n == 0 {
        cm = M::zeros(dm.nrows(), 0);
    }
    let (p, m_in) = (cm.nrows(), bm.ncols());
    if dm.nrows() == 1 && dm.ncols() == 1 && dm[(0, 0)] == 0.0 && (p, m_in) != (1, 1) {
        dm = M::zeros(p, m_in);
    }
    if bm.nrows() != n || cm.ncols() != n || dm.nrows() != p || dm.ncols() != m_in {
        return Err(merr!(
            "The A, B, C and D matrices must have compatible sizes."
        ));
    }
    let mut m = Model {
        kind: Kind::Ss,
        num: vec![],
        den: vec![],
        zpk: None,
        ss: Some(Ss {
            a: am,
            b: bm,
            c: cm,
            d: dm,
        }),
        ts: 0.0,
        delay: 0.0,
    };
    let mut i = 4;
    if a.len() > 4 && !a[4].is_text() {
        m.ts = a[4].scalar()?;
        i = 5;
    }
    apply_props(&mut m, &a[i.min(a.len())..])?;
    one(model_value(m))
}

fn frd_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let w = a[1].to_num()?.data;
    let r = if is_lti(&a[0]) {
        let m = obj_model(&a[0])?;
        w.iter()
            .map(|x| m.freq(*x))
            .collect::<MResult<Vec<C64>>>()?
    } else {
        a[0].to_cplx()?.data
    };
    if r.len() != w.len() {
        return Err(merr!(
            "The response data and frequency vector must have the same length."
        ));
    }
    let ts = a
        .get(2)
        .filter(|v| !v.is_text())
        .map(|v| v.scalar())
        .transpose()?
        .unwrap_or(0.0);
    one(Value::Obj(Box::new(Object::new(
        FRD,
        vec![
            ("ResponseData".into(), Value::complex(Array::col(r))),
            ("Frequency".into(), Value::col(w)),
            ("FrequencyUnit".into(), Value::chars("rad/TimeUnit")),
            ("Ts".into(), Value::num(ts)),
        ],
    ))))
}

/// Frequency response of a model or of frd data at `w`.
fn response(v: &Value, w: &[f64]) -> MResult<Vec<C64>> {
    if let Value::Obj(o) = v {
        if o.class == FRD {
            let r = o.get("ResponseData").unwrap().to_cplx()?.data;
            let f = o.get("Frequency").unwrap().to_num()?.data;
            return Ok(w
                .iter()
                .map(|x| {
                    f.iter()
                        .position(|y| (y - x).abs() <= 1e-9 * x.abs().max(1.0))
                        .map(|k| r[k])
                        .unwrap_or(c(f64::NAN, 0.0))
                })
                .collect());
        }
    }
    let m = obj_model(v)?;
    w.iter().map(|x| m.freq(*x)).collect()
}

// ---------------------------------------------------------------- registration

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("tf", tf_fn);
    b.insert("zpk", zpk_fn);
    b.insert("ss", ss_fn);
    b.insert("frd", frd_fn);
    b.insert("tfdata", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = obj_model(&a[0])?;
        let (n, d) = m.rat()?;
        let vec_form = a
            .get(1)
            .map(|v| v.text())
            .transpose()?
            .is_some_and(|t| t == "v");
        let wrap = |p: Vec<f64>| {
            if vec_form {
                Value::row(p)
            } else {
                Value::cell_row(vec![Value::row(p)])
            }
        };
        let len = n.len().max(d.len());
        let pad = |p: Vec<f64>| [vec![0.0; len - p.len()], p].concat();
        Ok(vec![wrap(pad(n)), wrap(pad(d)), Value::num(m.ts)])
    });
    b.insert("zpkdata", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = obj_model(&a[0])?;
        let (z, p, k) = zpk_of(&m)
            .ok_or_else(|| merr!("zpkdata needs a single-input, single-output model."))?;
        let vec_form = a
            .get(1)
            .map(|v| v.text())
            .transpose()?
            .is_some_and(|t| t == "v");
        let wrap = |r: Vec<C64>| {
            let v = Value::complex(Array::col(r));
            if vec_form {
                v
            } else {
                Value::cell_row(vec![v])
            }
        };
        Ok(vec![wrap(z), wrap(p), Value::num(k), Value::num(m.ts)])
    });
    b.insert("ssdata", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = obj_model(&a[0])?;
        let s = m.state_space();
        Ok(vec![
            mval(&s.a),
            mval(&s.b),
            mval(&s.c),
            mval(&s.d),
            Value::num(m.ts),
        ])
    });
    b.insert("pole", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::complex(Array::col(obj_model(&a[0])?.poles())))
    });
    b.insert("zero", |_, a, nargout| {
        check_args(&a, 1, 1)?;
        let m = obj_model(&a[0])?;
        let z = m.zeros()?;
        let mut out = vec![Value::complex(Array::col(z))];
        if nargout > 1 {
            out.push(Value::num(zpk_of(&m).map(|t| t.2).unwrap_or(0.0)));
        }
        Ok(out)
    });
    b.insert("dcgain", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = obj_model(&a[0])?;
        let s0 = if m.ts != 0.0 {
            c(1.0, 0.0)
        } else {
            c(0.0, 0.0)
        };
        let (n, d) = m.rat()?;
        let (nv, dv) = (peval(&n, s0), peval(&d, s0));
        let g = if dv.norm() == 0.0 {
            if nv.norm() == 0.0 {
                f64::NAN
            } else {
                f64::INFINITY.copysign(nv.re)
            }
        } else {
            (nv / dv).re
        };
        one(Value::num(g))
    });
    b.insert("evalfr", |_, a, _| {
        check_args(&a, 2, 2)?;
        let m = obj_model(&a[0])?;
        let s = a[1].to_cplx()?;
        one(Value::complex(
            s.map(|z| m.eval(*z).unwrap_or(c(f64::NAN, 0.0))),
        ))
    });
    b.insert("freqresp", |_, a, nargout| {
        check_args(&a, 1, 3)?;
        let w = match a.get(1) {
            Some(v) => v.to_num()?.data,
            None => default_freqs(&[obj_model(&a[0])?]),
        };
        let r = response(&a[0], &w)?;
        let mut out = vec![Value::complex(Array::col(r))];
        if nargout > 1 {
            out.push(Value::col(w));
        }
        Ok(out)
    });
    b.insert("series", |_, a, _| {
        check_args(&a, 2, 4)?;
        let (m1, m2) = (obj_model(&a[0])?, obj_model(&a[1])?);
        one(model_value(mul(&m2, &m1)?))
    });
    b.insert("parallel", |_, a, _| {
        check_args(&a, 2, 4)?;
        let (m1, m2) = (obj_model(&a[0])?, obj_model(&a[1])?);
        one(model_value(add(&m1, &m2, 1.0)?))
    });
    b.insert("feedback", feedback);
    b.insert("minreal", |it, a, _| {
        check_args(&a, 1, 2)?;
        let m = obj_model(&a[0])?;
        let tol = a
            .get(1)
            .map(|v| v.scalar())
            .transpose()?
            .unwrap_or(f64::EPSILON.sqrt());
        let r = minreal(&m, tol)?;
        let removed = m.poles().len() - r.poles().len();
        if removed > 0 {
            it.print(&format!(
                "{} state{} removed.\n",
                removed,
                if removed == 1 { "" } else { "s" }
            ));
        }
        one(model_value(r))
    });
    b.insert("order", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(obj_model(&a[0])?.state_space().a.nrows() as f64))
    });
    b.insert("isstable", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = obj_model(&a[0])?;
        let p = m.poles();
        let ok = if m.ts != 0.0 {
            p.iter().all(|z| z.norm() < 1.0)
        } else {
            p.iter().all(|z| z.re < 0.0)
        };
        one(bool_val(ok))
    });
    b.insert("isproper", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = obj_model(&a[0])?;
        if m.ss.is_some() {
            return one(bool_val(true));
        }
        one(bool_val(
            trim(m.num.clone()).len() <= trim(m.den.clone()).len(),
        ))
    });
    b.insert("isct", |_, a, _| one(bool_val(obj_model(&a[0])?.ts == 0.0)));
    b.insert("isdt", |_, a, _| one(bool_val(obj_model(&a[0])?.ts != 0.0)));
    b.insert("issiso", |_, a, _| {
        one(bool_val(obj_model(&a[0])?.is_siso()))
    });
    b.insert("pid", |_, a, _| {
        let k = |i: usize| -> MResult<f64> {
            a.get(i)
                .map(|v| v.scalar())
                .transpose()
                .map(|v| v.unwrap_or(0.0))
        };
        let (kp, ki, kd, tf) = (k(0)?, k(1)?, k(2)?, k(3)?);
        // Kp + Ki/s + Kd s/(Tf s + 1)
        let m = if tf > 0.0 {
            Model::rational(
                vec![kp * tf + kd, kp + ki * tf, ki],
                vec![tf, 1.0, 0.0],
                0.0,
            )
        } else {
            Model::rational(vec![kd, kp, ki], vec![1.0, 0.0], 0.0)
        };
        let m = if ki == 0.0 { minreal(&m, 1e-12)? } else { m };
        one(model_value(m))
    });
    b.insert("pidstd", |_, a, _| {
        let k = |i: usize, d: f64| -> MResult<f64> {
            a.get(i)
                .map(|v| v.scalar())
                .transpose()
                .map(|v| v.unwrap_or(d))
        };
        let (kp, ti, td, n) = (
            k(0, 1.0)?,
            k(1, f64::INFINITY)?,
            k(2, 0.0)?,
            k(3, f64::INFINITY)?,
        );
        let ki = if ti.is_finite() { kp / ti } else { 0.0 };
        let kd = kp * td;
        let tf = if n.is_finite() && td > 0.0 {
            td / n
        } else {
            0.0
        };
        let m = if tf > 0.0 {
            Model::rational(
                vec![kp * tf + kd, kp + ki * tf, ki],
                vec![tf, 1.0, 0.0],
                0.0,
            )
        } else {
            Model::rational(vec![kd, kp, ki], vec![1.0, 0.0], 0.0)
        };
        let m = if ki == 0.0 { minreal(&m, 1e-12)? } else { m };
        one(model_value(m))
    });
    b.insert("pade", pade);
    b.insert("c2d", c2d);
    b.insert("d2c", d2c);
    b.insert("damp", damp);
    b.insert("ctrb", |_, a, _| {
        let (am, bm) = ab_args(&a)?;
        let n = am.nrows();
        let mut cols = Vec::new();
        let mut blk = bm.clone();
        for _ in 0..n {
            cols.push(blk.clone());
            blk = &am * &blk;
        }
        one(mval(&hcat(&cols, n)))
    });
    b.insert("obsv", |_, a, _| {
        let (am, cm) = if a.len() == 1 {
            let s = obj_model(&a[0])?.state_space();
            (s.a, s.c)
        } else {
            check_args(&a, 2, 2)?;
            (
                crate::ops::to_dmatrix(&a[0].to_num()?),
                crate::ops::to_dmatrix(&a[1].to_num()?),
            )
        };
        let n = am.nrows();
        let mut rows = Vec::new();
        let mut blk = cm.clone();
        for _ in 0..n {
            rows.push(blk.clone());
            blk = &blk * &am;
        }
        let tall = hcat(
            &rows.iter().map(|r| r.transpose()).collect::<Vec<_>>(),
            cm.ncols(),
        )
        .transpose();
        one(mval(&tall))
    });
    b.insert("lyap", |_, a, _| {
        check_args(&a, 2, 3)?;
        let am = crate::ops::to_dmatrix(&a[0].to_num()?);
        let q = crate::ops::to_dmatrix(&a[1].to_num()?);
        if a.len() == 3 {
            // Sylvester: A X + X B + C = 0
            let bm = q;
            let cm = crate::ops::to_dmatrix(&a[2].to_num()?);
            return one(mval(&sylvester(&am, &bm, &cm)?));
        }
        one(mval(&sylvester(&am, &am.transpose(), &q)?))
    });
    b.insert("dlyap", |_, a, _| {
        check_args(&a, 2, 2)?;
        let am = crate::ops::to_dmatrix(&a[0].to_num()?);
        let q = crate::ops::to_dmatrix(&a[1].to_num()?);
        one(mval(&dlyap(&am, &q)?))
    });
    b.insert("gram", |_, a, _| {
        check_args(&a, 2, 2)?;
        let m = obj_model(&a[0])?;
        let s = m.state_space();
        let kind = a[1].text()?;
        let (am, q) = if kind == "o" {
            (s.a.transpose(), s.c.transpose() * &s.c)
        } else {
            (s.a.clone(), &s.b * s.b.transpose())
        };
        let g = if m.ts != 0.0 {
            dlyap(&am, &q)?
        } else {
            sylvester(&am, &am.transpose(), &q)?
        };
        one(mval(&g))
    });
    b.insert("care", |_, a, nargout| riccati(&a, nargout, false));
    b.insert("dare", |_, a, nargout| riccati(&a, nargout, true));
    b.insert("lqr", |_, a, nargout| lqr(&a, nargout, false));
    b.insert("dlqr", |_, a, nargout| lqr(&a, nargout, true));
    b.insert("place", place);
    b.insert("acker", place);
    b.insert("step", |it, a, n| time_response(it, a, n, Resp::Step));
    b.insert("impulse", |it, a, n| time_response(it, a, n, Resp::Impulse));
    b.insert("initial", |it, a, n| time_response(it, a, n, Resp::Initial));
    b.insert("lsim", lsim);
    b.insert("stepinfo", stepinfo);
    b.insert("bode", |it, a, n| bode(it, a, n, false));
    b.insert("bodemag", |it, a, n| bode(it, a, n, true));
    b.insert("nyquist", nyquist);
    b.insert("nichols", nichols);
    b.insert("margin", margin);
    b.insert("allmargin", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (gm, pm, wcg, wcp) = margins(&a[0])?;
        one(Value::Struct(StructArr::scalar(vec![
            ("GainMargin".into(), Value::num(gm)),
            ("GMFrequency".into(), Value::num(wcg)),
            ("PhaseMargin".into(), Value::num(pm)),
            ("PMFrequency".into(), Value::num(wcp)),
            (
                "Stable".into(),
                bool_val(obj_model(&a[0])?.poles().iter().all(|p| p.re < 0.0)),
            ),
        ])))
    });
    b.insert("rlocus", rlocus);
    b.insert("pzmap", pzmap);
    // Versions of general functions used when an argument is a model.
    let ob = &mut it.lti_builtins;
    ob.insert("inv", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(model_value(inv(&obj_model(&a[0])?)?))
    });
    ob.insert("plus", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(lti_binary(BinOp::Add, &a[0], &a[1])?)
    });
    ob.insert("minus", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(lti_binary(BinOp::Sub, &a[0], &a[1])?)
    });
    ob.insert("mtimes", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(lti_binary(BinOp::MatMul, &a[0], &a[1])?)
    });
    ob.insert("mrdivide", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(lti_binary(BinOp::MatDiv, &a[0], &a[1])?)
    });
    ob.insert("size", |_, a, _| {
        let m = obj_model(&a[0])?;
        let s = m.state_space();
        let (p, mi) = (s.c.nrows().max(s.d.nrows()), s.b.ncols().max(s.d.ncols()));
        one(Value::row(vec![p as f64, mi as f64]))
    });
}

fn hcat(blocks: &[M], rows: usize) -> M {
    let cols: usize = blocks.iter().map(|b| b.ncols()).sum();
    let mut out = M::zeros(rows, cols);
    let mut j = 0;
    for b in blocks {
        out.view_mut((0, j), (rows, b.ncols())).copy_from(b);
        j += b.ncols();
    }
    out
}

fn ab_args(a: &[Value]) -> MResult<(M, M)> {
    if a.len() == 1 {
        let s = obj_model(&a[0])?.state_space();
        return Ok((s.a, s.b));
    }
    check_args(a, 2, 2)?;
    Ok((
        crate::ops::to_dmatrix(&a[0].to_num()?),
        crate::ops::to_dmatrix(&a[1].to_num()?),
    ))
}

fn feedback(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 3)?;
    let g = obj_model(&a[0])?;
    let h = match a.get(1) {
        Some(v) => obj_model(v)?,
        None => Model::gain(1.0),
    };
    let sign = match a.get(2) {
        Some(v) if v.scalar()? > 0.0 => 1.0,
        _ => -1.0,
    };
    let ts = common_ts(&g, &h)?;
    let kind = result_kind(&g, &h);
    let mut m = if kind == Kind::Ss {
        Model {
            kind,
            num: vec![],
            den: vec![],
            zpk: None,
            ss: Some(ss_feedback(&g.state_space(), &h.state_space(), sign)?),
            ts,
            delay: 0.0,
        }
    } else {
        let (ng, dg) = g.rat()?;
        let (nh, dh) = h.rat()?;
        // G/(1 - sign*G*H)
        let num = conv(&ng, &dh);
        let den = padd(&conv(&dg, &dh), &pscale(&conv(&ng, &nh), -sign));
        let mut m = Model::rational(num, den, ts);
        m.kind = kind;
        m
    };
    m.ts = ts;
    if g.delay > 0.0 || h.delay > 0.0 {
        m.delay = g.delay;
    }
    one(model_value(m))
}

/// Cancel pole/zero pairs closer than `tol` (relative).
fn minreal(m: &Model, tol: f64) -> MResult<Model> {
    let (n, d) = m.rat()?;
    let mut z = roots(&n);
    let mut p = roots(&d);
    let k = n.first().copied().unwrap_or(0.0) / d.first().copied().unwrap_or(1.0);
    let mut i = 0;
    while i < z.len() {
        if let Some(j) = p
            .iter()
            .position(|q| (*q - z[i]).norm() <= tol * z[i].norm().max(1.0))
        {
            p.remove(j);
            z.remove(i);
        } else {
            i += 1;
        }
    }
    let mut r = Model::rational(pscale(&poly_real(&z), k), poly_real(&p), m.ts);
    r.kind = m.kind;
    r.delay = m.delay;
    if m.kind == Kind::Zpk {
        r.zpk = Some((z, p, k));
    }
    if m.kind == Kind::Ss {
        r = r.with_kind(Kind::Ss);
    }
    Ok(r)
}

/// `[num, den] = pade(T, N)` or `sysx = pade(sys, N)`.
fn pade(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 2, 2)?;
    let n = a[1].scalar()? as usize;
    let coeffs = |t: f64| -> (Vec<f64>, Vec<f64>) {
        // Coefficients of the [n/n] Padé approximant of exp(-sT), descending powers.
        let mut num = vec![0.0; n + 1];
        let mut den = vec![0.0; n + 1];
        let fact = |k: usize| (1..=k).map(|x| x as f64).product::<f64>();
        for k in 0..=n {
            let ck = fact(2 * n - k) * fact(n) / (fact(2 * n) * fact(k) * fact(n - k));
            num[n - k] = ck * (-t).powi(k as i32);
            den[n - k] = ck * t.powi(k as i32);
        }
        (num, den)
    };
    if is_lti(&a[0]) {
        let m = obj_model(&a[0])?;
        if m.delay == 0.0 {
            return one(a[0].clone());
        }
        let (pn, pd) = coeffs(m.delay);
        let mut base = m.clone();
        base.delay = 0.0;
        let r = mul(&base, &Model::rational(pn, pd, 0.0))?;
        return one(model_value(r.with_kind(m.kind)));
    }
    let t = a[0].scalar()?;
    let (num, den) = coeffs(t);
    if nargout == 0 {
        let m = Model::rational(num, den, 0.0);
        return time_response(it, vec![model_value(m)], 0, Resp::Step);
    }
    Ok(vec![Value::row(num), Value::row(den)])
}

// ---------------------------------------------------------------- matrix helpers

fn expm(a: &M) -> M {
    a.clone().exp()
}

/// Solve A X + X B + C = 0 by vectorization.
fn sylvester(a: &M, b: &M, cm: &M) -> MResult<M> {
    let n = a.nrows();
    let m = b.nrows();
    let k = M::identity(m, m).kronecker(a) + b.transpose().kronecker(&M::identity(n, n));
    let rhs = -DMatrix::from_column_slice(n * m, 1, cm.as_slice());
    let x = k
        .lu()
        .solve(&rhs)
        .ok_or_else(|| merr!("The Lyapunov/Sylvester equation has no unique solution."))?;
    Ok(DMatrix::from_column_slice(n, m, x.as_slice()))
}

/// Solve A X A' - X + Q = 0.
fn dlyap(a: &M, q: &M) -> MResult<M> {
    let n = a.nrows();
    let k = a.kronecker(a) - M::identity(n * n, n * n);
    let rhs = -DMatrix::from_column_slice(n * n, 1, q.as_slice());
    let x = k
        .lu()
        .solve(&rhs)
        .ok_or_else(|| merr!("The discrete Lyapunov equation has no unique solution."))?;
    Ok(DMatrix::from_column_slice(n, n, x.as_slice()))
}

/// Stabilizing solution of the continuous (matrix sign function) or discrete
/// (doubling algorithm) algebraic Riccati equation.
fn are(a: &M, b: &M, q: &M, r: &M, discrete: bool) -> MResult<M> {
    let n = a.nrows();
    let rinv = r
        .clone()
        .try_inverse()
        .ok_or_else(|| merr!("R must be invertible."))?;
    let g = b * &rinv * b.transpose();
    if discrete {
        let (mut ak, mut gk, mut hk) = (a.clone(), g, q.clone());
        for _ in 0..200 {
            let w = (M::identity(n, n) + &gk * &hk)
                .try_inverse()
                .ok_or_else(|| merr!("The Riccati iteration broke down."))?;
            let a1 = &ak * &w * &ak;
            let g1 = &gk + &ak * &w * &gk * ak.transpose();
            let h1 = &hk + ak.transpose() * &hk * &w * &ak;
            let done = (&h1 - &hk).norm() <= 1e-13 * h1.norm().max(1.0);
            ak = a1;
            gk = g1;
            hk = h1;
            if done {
                break;
            }
        }
        return Ok((&hk + hk.transpose()) * 0.5);
    }
    let mut z = M::zeros(2 * n, 2 * n);
    z.view_mut((0, 0), (n, n)).copy_from(a);
    z.view_mut((0, n), (n, n)).copy_from(&(-&g));
    z.view_mut((n, 0), (n, n)).copy_from(&(-q));
    z.view_mut((n, n), (n, n)).copy_from(&(-a.transpose()));
    for _ in 0..100 {
        let zi = z
            .clone()
            .try_inverse()
            .ok_or_else(|| merr!("The Hamiltonian matrix is singular; no stabilizing solution."))?;
        // Determinant scaling speeds up convergence.
        let det = z.determinant().abs();
        let mu = if det > 0.0 {
            det.powf(-1.0 / (2 * n) as f64)
        } else {
            1.0
        };
        let z1 = (&z * mu + &zi / mu) * 0.5;
        let done = (&z1 - &z).norm() <= 1e-12 * z1.norm();
        z = z1;
        if done {
            break;
        }
    }
    let w11 = z.view((0, 0), (n, n)).clone_owned();
    let w12 = z.view((0, n), (n, n)).clone_owned();
    let w21 = z.view((n, 0), (n, n)).clone_owned();
    let w22 = z.view((n, n), (n, n)).clone_owned();
    let i = M::identity(n, n);
    // [W12; W22 + I] X = -[W11 + I; W21]
    let mut lhs = M::zeros(2 * n, n);
    lhs.view_mut((0, 0), (n, n)).copy_from(&w12);
    lhs.view_mut((n, 0), (n, n)).copy_from(&(&w22 + &i));
    let mut rhs = M::zeros(2 * n, n);
    rhs.view_mut((0, 0), (n, n)).copy_from(&(-(&w11 + &i)));
    rhs.view_mut((n, 0), (n, n)).copy_from(&(-&w21));
    let x = lhs
        .svd(true, true)
        .solve(&rhs, 1e-14)
        .map_err(|e| merr!("{}", e))?;
    Ok((&x + x.transpose()) * 0.5)
}

fn riccati(a: &[Value], nargout: usize, discrete: bool) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let am = crate::ops::to_dmatrix(&a[0].to_num()?);
    let bm = crate::ops::to_dmatrix(&a[1].to_num()?);
    let n = am.nrows();
    let q = match a.get(2) {
        Some(v) => crate::ops::to_dmatrix(&v.to_num()?),
        None => M::identity(n, n),
    };
    let r = match a.get(3) {
        Some(v) => crate::ops::to_dmatrix(&v.to_num()?),
        None => M::identity(bm.ncols(), bm.ncols()),
    };
    let x = are(&am, &bm, &q, &r, discrete)?;
    let rinv = r.clone().try_inverse().unwrap();
    let k = if discrete {
        (&r + bm.transpose() * &x * &bm)
            .try_inverse()
            .unwrap_or(rinv)
            * bm.transpose()
            * &x
            * &am
    } else {
        &rinv * bm.transpose() * &x
    };
    let cl = eigenvalues(&(&am - &bm * &k));
    let mut out = vec![mval(&x)];
    if nargout > 1 {
        out.push(Value::complex(Array::col(cl)));
    }
    if nargout > 2 {
        out.push(mval(&k));
    }
    Ok(out)
}

fn lqr(a: &[Value], nargout: usize, discrete: bool) -> R {
    let (am, bm, q, r, nm) = if a.len() >= 3 && is_lti(&a[0]) {
        let s = obj_model(&a[0])?.state_space();
        let q = crate::ops::to_dmatrix(&a[1].to_num()?);
        let r = crate::ops::to_dmatrix(&a[2].to_num()?);
        let nm = a
            .get(3)
            .map(|v| v.to_num())
            .transpose()?
            .map(|v| crate::ops::to_dmatrix(&v));
        (s.a, s.b, q, r, nm)
    } else {
        if a.len() < 4 {
            return Err(merr!("Not enough input arguments."));
        }
        let m = |i: usize| -> MResult<M> { Ok(crate::ops::to_dmatrix(&a[i].to_num()?)) };
        let nm = a
            .get(4)
            .map(|v| v.to_num())
            .transpose()?
            .map(|v| crate::ops::to_dmatrix(&v));
        (m(0)?, m(1)?, m(2)?, m(3)?, nm)
    };
    // Cross term: shift to the standard problem.
    let rinv = r
        .clone()
        .try_inverse()
        .ok_or_else(|| merr!("R must be invertible."))?;
    let (a2, q2) = match &nm {
        Some(n) => (
            &am - &bm * &rinv * n.transpose(),
            &q - n * &rinv * n.transpose(),
        ),
        None => (am.clone(), q.clone()),
    };
    let x = are(&a2, &bm, &q2, &r, discrete)?;
    let k = if discrete {
        let lhs = (&r + bm.transpose() * &x * &bm)
            .try_inverse()
            .ok_or_else(|| merr!("Singular gain equation."))?;
        let cross = nm
            .as_ref()
            .map(|n| n.transpose())
            .unwrap_or_else(|| M::zeros(bm.ncols(), am.nrows()));
        lhs * (bm.transpose() * &x * &am + cross)
    } else {
        let cross = nm
            .as_ref()
            .map(|n| n.transpose())
            .unwrap_or_else(|| M::zeros(bm.ncols(), am.nrows()));
        &rinv * (bm.transpose() * &x + cross)
    };
    let mut out = vec![mval(&k)];
    if nargout > 1 {
        out.push(mval(&x));
    }
    if nargout > 2 {
        out.push(Value::complex(Array::col(eigenvalues(&(&am - &bm * &k)))));
    }
    Ok(out)
}

/// Pole placement (Ackermann's formula for one input; a least-squares eigenvector
/// method otherwise).
fn place(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 3, 3)?;
    let am = crate::ops::to_dmatrix(&a[0].to_num()?);
    let bm = crate::ops::to_dmatrix(&a[1].to_num()?);
    let p = a[2].to_cplx()?.data;
    let n = am.nrows();
    if p.len() != n {
        return Err(merr!(
            "The number of poles must equal the number of states."
        ));
    }
    if bm.ncols() != 1 {
        return Err(merr!(
            "Pole placement with several inputs is not supported yet."
        ));
    }
    let mut cols = Vec::new();
    let mut blk = bm.clone();
    for _ in 0..n {
        cols.push(blk.clone());
        blk = &am * &blk;
    }
    let ctrb = hcat(&cols, n);
    let ci = ctrb
        .try_inverse()
        .ok_or_else(|| merr!("The system is not controllable; the poles cannot be placed."))?;
    let coeffs = poly_real(&p);
    // phi(A) = A^n + c1 A^(n-1) + ... + cn I
    let mut phi = M::zeros(n, n);
    let mut pw = M::identity(n, n);
    for k in (0..=n).rev() {
        phi += &pw * coeffs[k];
        pw = &pw * &am;
    }
    let mut last = M::zeros(1, n);
    last[(0, n - 1)] = 1.0;
    one(mval(&(last * ci * phi)))
}

// ---------------------------------------------------------------- discretization

fn c2d(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 4)?;
    let m = obj_model(&a[0])?;
    if m.ts != 0.0 {
        return Err(merr!("The model is already discrete."));
    }
    let ts = a[1].scalar()?;
    let method = a
        .get(2)
        .filter(|v| v.is_text())
        .map(|v| v.text())
        .transpose()?
        .unwrap_or_else(|| "zoh".into());
    let mut out = match method.as_str() {
        "zoh" | "foh" => {
            let s = m.state_space();
            let n = s.a.nrows();
            let mi = s.b.ncols();
            if method == "zoh" {
                let mut aug = M::zeros(n + mi, n + mi);
                aug.view_mut((0, 0), (n, n)).copy_from(&(&s.a * ts));
                aug.view_mut((0, n), (n, mi)).copy_from(&(&s.b * ts));
                let e = expm(&aug);
                Model {
                    kind: Kind::Ss,
                    num: vec![],
                    den: vec![],
                    zpk: None,
                    ss: Some(Ss {
                        a: e.view((0, 0), (n, n)).clone_owned(),
                        b: e.view((0, n), (n, mi)).clone_owned(),
                        c: s.c.clone(),
                        d: s.d.clone(),
                    }),
                    ts,
                    delay: 0.0,
                }
            } else {
                let (phi, g1, g2) = foh_matrices(&s, ts);
                // x[k+1] = Phi x + (G1 - G2) u[k] + G2 u[k+1]; change of state removes u[k+1].
                let bd = &g1 - &g2 + &phi * &g2;
                let cd = s.c.clone();
                let dd = &s.d + &s.c * &g2;
                Model {
                    kind: Kind::Ss,
                    num: vec![],
                    den: vec![],
                    zpk: None,
                    ss: Some(Ss {
                        a: phi,
                        b: bd,
                        c: cd,
                        d: dd,
                    }),
                    ts,
                    delay: 0.0,
                }
            }
        }
        "tustin" | "bilinear" => {
            let (n, d) = m.rat()?;
            let (nz, dz) = bilinear(&n, &d, 2.0 / ts);
            Model::rational(nz, dz, ts)
        }
        "matched" => {
            let z: Vec<C64> = m.zeros()?.iter().map(|q| (q * ts).exp()).collect();
            let p: Vec<C64> = m.poles().iter().map(|q| (q * ts).exp()).collect();
            let mut r = Model::rational(poly_real(&z), poly_real(&p), ts);
            // Match the DC gain.
            let g0 = m.eval(c(0.0, 0.0))?;
            let g1 = r.eval(c(1.0, 0.0))?;
            if g1.norm() > 0.0 && g0.norm().is_finite() {
                r.num = pscale(&r.num, (g0 / g1).re);
            }
            r
        }
        "impulse" => {
            let s = m.state_space();
            let e = expm(&(&s.a * ts));
            Model {
                kind: Kind::Ss,
                num: vec![],
                den: vec![],
                zpk: None,
                ss: Some(Ss {
                    a: e.clone(),
                    b: &e * &s.b * ts,
                    c: s.c.clone(),
                    d: &s.d * ts,
                }),
                ts,
                delay: 0.0,
            }
        }
        other => return Err(merr!("Unknown discretization method '{}'.", other)),
    };
    // Delays become whole samples (fractions are rounded).
    let samples = (m.delay / ts).round();
    out.ts = ts;
    out.delay = samples;
    let out = if out.delay > 0.0 && m.kind != Kind::Ss {
        let mut r = mul(
            &out,
            &Model::rational(
                vec![1.0],
                [vec![1.0], vec![0.0; samples as usize]].concat(),
                ts,
            ),
        )?;
        r.delay = 0.0;
        r
    } else {
        out
    };
    one(model_value(out.with_kind(m.kind)))
}

fn foh_matrices(s: &Ss, h: f64) -> (M, M, M) {
    let n = s.a.nrows();
    let mi = s.b.ncols();
    let size = n + 2 * mi;
    let mut aug = M::zeros(size, size);
    aug.view_mut((0, 0), (n, n)).copy_from(&(&s.a * h));
    aug.view_mut((0, n), (n, mi)).copy_from(&(&s.b * h));
    aug.view_mut((n, n + mi), (mi, mi))
        .copy_from(&M::identity(mi, mi));
    let e = expm(&aug);
    (
        e.view((0, 0), (n, n)).clone_owned(),
        e.view((0, n), (n, mi)).clone_owned(),
        e.view((0, n + mi), (n, mi)).clone_owned(),
    )
}

/// Substitute s = k (z-1)/(z+1) into num/den.
fn bilinear(num: &[f64], den: &[f64], k: f64) -> (Vec<f64>, Vec<f64>) {
    let order = num.len().max(den.len()) - 1;
    let map = |p: &[f64]| -> Vec<f64> {
        let mut out = vec![0.0];
        let deg = p.len() - 1;
        for (i, coef) in p.iter().enumerate() {
            let pw = deg - i;
            // coef * k^pw (z-1)^pw (z+1)^(order-pw)
            let mut term = vec![coef * k.powi(pw as i32)];
            for _ in 0..pw {
                term = conv(&term, &[1.0, -1.0]);
            }
            for _ in 0..(order - pw) {
                term = conv(&term, &[1.0, 1.0]);
            }
            out = padd(&out, &term);
        }
        out
    };
    let n = map(num);
    let d = map(den);
    let lead = d[0];
    (pscale(&n, 1.0 / lead), pscale(&d, 1.0 / lead))
}

fn d2c(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = obj_model(&a[0])?;
    if m.ts <= 0.0 {
        return Err(merr!(
            "d2c needs a discrete model with a known sample time."
        ));
    }
    let ts = m.ts;
    let method = a
        .get(1)
        .map(|v| v.text())
        .transpose()?
        .unwrap_or_else(|| "zoh".into());
    let out = match method.as_str() {
        "tustin" | "bilinear" => {
            // z = (1 + s T/2)/(1 - s T/2)
            let (n, d) = m.rat()?;
            let order = n.len().max(d.len()) - 1;
            let map = |p: &[f64]| -> Vec<f64> {
                let deg = p.len() - 1;
                let mut out = vec![0.0];
                for (i, coef) in p.iter().enumerate() {
                    let pw = deg - i;
                    let mut term = vec![*coef];
                    for _ in 0..pw {
                        term = conv(&term, &[ts / 2.0, 1.0]);
                    }
                    for _ in 0..(order - pw) {
                        term = conv(&term, &[-ts / 2.0, 1.0]);
                    }
                    out = padd(&out, &term);
                }
                out
            };
            let (nn, dd) = (map(&n), map(&d));
            let lead = dd[0];
            Model::rational(pscale(&nn, 1.0 / lead), pscale(&dd, 1.0 / lead), 0.0)
        }
        _ => {
            let s = m.state_space();
            let n = s.a.nrows();
            let mi = s.b.ncols();
            let mut aug = M::identity(n + mi, n + mi);
            aug.view_mut((0, 0), (n, n)).copy_from(&s.a);
            aug.view_mut((0, n), (n, mi)).copy_from(&s.b);
            let l = logm(&aug)? / ts;
            Model {
                kind: Kind::Ss,
                num: vec![],
                den: vec![],
                zpk: None,
                ss: Some(Ss {
                    a: l.view((0, 0), (n, n)).clone_owned(),
                    b: l.view((0, n), (n, mi)).clone_owned(),
                    c: s.c.clone(),
                    d: s.d.clone(),
                }),
                ts: 0.0,
                delay: 0.0,
            }
        }
    };
    one(model_value(out.with_kind(m.kind)))
}

/// Matrix logarithm by inverse scaling and squaring with a Padé-free series.
fn logm(a: &M) -> MResult<M> {
    let n = a.nrows();
    let i = M::identity(n, n);
    let mut x = a.clone();
    let mut k = 0;
    // Square roots until close to the identity (Denman–Beavers iteration).
    while (&x - &i).norm() > 0.25 && k < 40 {
        let (mut y, mut z) = (x.clone(), i.clone());
        for _ in 0..60 {
            let yi = y
                .clone()
                .try_inverse()
                .ok_or_else(|| merr!("logm: matrix is singular."))?;
            let zi = z
                .clone()
                .try_inverse()
                .ok_or_else(|| merr!("logm: matrix is singular."))?;
            let y1 = (&y + &zi) * 0.5;
            let z1 = (&z + &yi) * 0.5;
            let done = (&y1 - &y).norm() <= 1e-15 * y1.norm();
            y = y1;
            z = z1;
            if done {
                break;
            }
        }
        x = y;
        k += 1;
    }
    // log(I + E) series.
    let e = &x - &i;
    let mut term = e.clone();
    let mut sum = M::zeros(n, n);
    for j in 1..200 {
        let t = &term / j as f64;
        if j % 2 == 1 {
            sum += &t;
        } else {
            sum -= &t;
        }
        if t.norm() < 1e-17 {
            break;
        }
        term = &term * &e;
    }
    Ok(sum * 2f64.powi(k))
}

fn damp(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let (p, ts) = if is_lti(&a[0]) {
        let m = obj_model(&a[0])?;
        (m.poles(), m.ts)
    } else {
        let v = a[0].to_cplx()?;
        let ts = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
        if v.rows == v.cols && v.rows > 1 {
            (eigenvalues(&crate::ops::to_dmatrix(&v.map(|z| z.re))), ts)
        } else {
            (v.data, ts)
        }
    };
    let mut p = p;
    p.sort_by(|x, y| {
        x.norm()
            .partial_cmp(&y.norm())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rows = Vec::new();
    for z in &p {
        // Discrete poles map to the equivalent continuous ones.
        let s = if ts > 0.0 { z.ln() / ts } else { *z };
        let wn = s.norm();
        let zeta = if wn == 0.0 {
            -1.0f64.copysign(s.re)
        } else {
            -s.re / wn
        };
        let tau = if s.re == 0.0 {
            f64::INFINITY
        } else {
            -1.0 / s.re
        };
        rows.push((wn, zeta, tau));
    }
    if nargout == 0 {
        let mut s = String::from("                                                                     \n         Pole              Damping       Frequency      Time Constant  \n                                       (rad/seconds)      (seconds)    \n                                                                       \n");
        for (z, (wn, zeta, tau)) in p.iter().zip(&rows) {
            let ps = if z.im == 0.0 {
                display::fmt_g(z.re, 5, true)
            } else {
                format!(
                    "{} {} {}i",
                    display::fmt_g(z.re, 3, true),
                    if z.im < 0.0 { '-' } else { '+' },
                    display::fmt_g(z.im.abs(), 3, true)
                )
            };
            s.push_str(&format!(
                " {:>20}    {:>12}    {:>12}    {:>12}\n",
                ps,
                display::fmt_g(*zeta, 3, true),
                display::fmt_g(*wn, 3, true),
                display::fmt_g(*tau, 3, true)
            ));
        }
        s.push('\n');
        it.print(&s);
        return none();
    }
    Ok(vec![
        Value::col(rows.iter().map(|r| r.0).collect()),
        Value::col(rows.iter().map(|r| r.1).collect()),
        Value::complex(Array::col(p)),
    ])
}

// ---------------------------------------------------------------- time responses

#[derive(Clone, Copy, PartialEq)]
enum Resp {
    Step,
    Impulse,
    Initial,
}

/// Default simulation horizon from the slowest dynamics.
fn default_tfinal(m: &Model) -> f64 {
    let p = m.poles();
    if m.ts != 0.0 {
        let ts = m.ts.abs();
        let slow = p
            .iter()
            .filter(|z| z.norm() < 1.0 && z.norm() > 0.0)
            .map(|z| -z.norm().ln() / ts)
            .fold(f64::INFINITY, f64::min);
        let n = if slow.is_finite() && slow > 0.0 {
            (6.0 / slow / ts).ceil()
        } else {
            50.0
        };
        return (n.clamp(10.0, 2000.0)) * ts + m.delay * ts;
    }
    let stable: Vec<f64> = p.iter().filter(|z| z.re < 0.0).map(|z| -z.re).collect();
    let slow = stable.iter().copied().fold(f64::INFINITY, f64::min);
    let unstable = p.iter().any(|z| z.re >= 0.0);
    let mut t = if slow.is_finite() { 6.0 / slow } else { 10.0 };
    if unstable {
        let fast = p
            .iter()
            .filter(|z| z.re > 0.0)
            .map(|z| z.re)
            .fold(0.0, f64::max);
        t = if fast > 0.0 {
            (3.0 / fast).min(t)
        } else {
            t.max(10.0)
        };
    }
    // Lightly damped oscillations need a few periods.
    for z in &p {
        if z.im.abs() > 0.0 && z.re < 0.0 {
            t = t.max((2.0 * std::f64::consts::PI / z.im.abs()) * 2.0);
        }
    }
    let t = t + m.delay;
    // Round to a tidy value.
    let mag = 10f64.powf(t.log10().floor());
    (t / mag).ceil() * mag
}

/// Simulate a state-space model on time grid `t` (uniform from 0 for continuous
/// models). `input(k)` gives the input vector at t[k]; returns outputs and states.
fn simulate(
    s: &Ss,
    ts: f64,
    t: &[f64],
    input: &dyn Fn(usize) -> Vec<f64>,
    x0: &[f64],
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let n = s.a.nrows();
    let mi = s.b.ncols();
    let mut x = DMatrix::from_column_slice(
        n,
        1,
        &x0.iter()
            .copied()
            .chain(std::iter::repeat(0.0))
            .take(n)
            .collect::<Vec<_>>(),
    );
    let mut ys = Vec::with_capacity(t.len());
    let mut xs = Vec::with_capacity(t.len());
    let uvec = |k: usize| DMatrix::from_column_slice(mi, 1, &input(k));
    let mut cache: Option<(f64, M, M, M)> = None;
    for k in 0..t.len() {
        let u = uvec(k);
        let y = &s.c * &x + &s.d * &u;
        ys.push(y.iter().copied().collect());
        xs.push(x.iter().copied().collect());
        if k + 1 == t.len() {
            break;
        }
        if ts != 0.0 {
            x = &s.a * &x + &s.b * &u;
        } else {
            let h = t[k + 1] - t[k];
            let fresh = cache
                .as_ref()
                .is_none_or(|(hc, ..)| (hc - h).abs() > 1e-12 * h.abs());
            if fresh {
                let (phi, g1, g2) = foh_matrices(s, h);
                cache = Some((h, phi, g1, g2));
            }
            let (_, phi, g1, g2) = cache.as_ref().unwrap();
            let u1 = uvec(k + 1);
            x = phi * &x + (g1 - g2) * &u + g2 * &u1;
        }
    }
    (ys, xs)
}

fn time_grid(m: &Model, spec: Option<&Value>) -> MResult<Vec<f64>> {
    let ts = if m.ts < 0.0 { 1.0 } else { m.ts };
    Ok(match spec {
        Some(v) if v.numel() > 1 => v.to_num()?.data,
        Some(v) => {
            let tf = v.scalar()?;
            if ts > 0.0 {
                (0..=((tf / ts).floor() as usize))
                    .map(|k| k as f64 * ts)
                    .collect()
            } else {
                (0..=1000).map(|k| tf * k as f64 / 1000.0).collect()
            }
        }
        None => {
            let tf = default_tfinal(m);
            if ts > 0.0 {
                (0..=((tf / ts).round() as usize))
                    .map(|k| k as f64 * ts)
                    .collect()
            } else {
                (0..=1000).map(|k| tf * k as f64 / 1000.0).collect()
            }
        }
    })
}

/// Response of one model; returns (t, y[output][time], x[time][state]) per input channel.
fn response_of(
    m: &Model,
    kind: Resp,
    t: &[f64],
    x0: &[f64],
) -> Vec<(Vec<Vec<f64>>, Vec<Vec<f64>>)> {
    let s = m.state_space();
    let mi = s.b.ncols().max(1);
    let ts = if m.ts < 0.0 { 1.0 } else { m.ts };
    let delay_t = if m.ts != 0.0 { m.delay * ts } else { m.delay };
    let mut out = Vec::new();
    let channels = if kind == Resp::Initial { 1 } else { mi };
    for ch in 0..channels {
        // Shift by the delay: simulate on t - delay and hold zero before it.
        let shifted: Vec<f64> = t.iter().map(|x| x - delay_t).collect();
        let start = shifted.iter().position(|x| *x >= -1e-12).unwrap_or(t.len());
        let tt: Vec<f64> = shifted[start..].to_vec();
        let (mut y, mut x) = match kind {
            Resp::Step => simulate(
                &s,
                m.ts,
                &tt,
                &|_| {
                    let mut u = vec![0.0; mi];
                    u[ch] = 1.0;
                    u
                },
                &[],
            ),
            Resp::Impulse => {
                if m.ts != 0.0 {
                    // Unit-area pulse of one sample.
                    simulate(
                        &s,
                        m.ts,
                        &tt,
                        &|k| {
                            let mut u = vec![0.0; mi];
                            if k == 0 {
                                u[ch] = 1.0 / ts;
                            }
                            u
                        },
                        &[],
                    )
                } else {
                    let b: Vec<f64> = s.b.column(ch).iter().copied().collect();
                    let free = Ss {
                        a: s.a.clone(),
                        b: M::zeros(s.a.nrows(), 1),
                        c: s.c.clone(),
                        d: M::zeros(s.c.nrows(), 1),
                    };
                    simulate(&free, 0.0, &tt, &|_| vec![0.0], &b)
                }
            }
            Resp::Initial => {
                let free = Ss {
                    a: s.a.clone(),
                    b: M::zeros(s.a.nrows(), 1),
                    c: s.c.clone(),
                    d: M::zeros(s.c.nrows(), 1),
                };
                simulate(&free, m.ts, &tt, &|_| vec![0.0], x0)
            }
        };
        let p = s.c.nrows();
        let mut ypad = vec![vec![0.0; p]; start];
        ypad.append(&mut y);
        let mut xpad = vec![vec![0.0; s.a.nrows()]; start];
        xpad.append(&mut x);
        out.push((ypad, xpad));
    }
    out
}

/// Response at arbitrary times: continuous models given a grid that is not uniform
/// from zero are simulated on a fine uniform grid and interpolated.
fn sampled_response(
    m: &Model,
    kind: Resp,
    t: &[f64],
    x0: &[f64],
) -> Vec<(Vec<Vec<f64>>, Vec<Vec<f64>>)> {
    let uniform = t.len() < 2
        || (t[0].abs() < 1e-12 && {
            let h = t[1] - t[0];
            t.windows(2)
                .all(|w| ((w[1] - w[0]) - h).abs() <= 1e-9 * h.abs().max(1e-12))
        });
    if m.ts != 0.0 || uniform {
        return response_of(m, kind, t, x0);
    }
    let tmax = t.iter().copied().fold(0.0f64, f64::max);
    let n = 4000;
    let grid: Vec<f64> = (0..=n).map(|k| tmax * k as f64 / n as f64).collect();
    let interp = |rows: &[Vec<f64>]| -> Vec<Vec<f64>> {
        t.iter()
            .map(|&ti| {
                let p = (ti.max(0.0) / tmax.max(1e-300) * n as f64).min(n as f64);
                let k = (p.floor() as usize).min(n - 1);
                let f = p - k as f64;
                rows[k]
                    .iter()
                    .zip(&rows[k + 1])
                    .map(|(a, b)| a * (1.0 - f) + b * f)
                    .collect()
            })
            .collect()
    };
    response_of(m, kind, &grid, x0)
        .into_iter()
        .map(|(y, x)| (interp(&y), interp(&x)))
        .collect()
}

fn split_systems(a: &[Value]) -> (Vec<(Value, Option<String>)>, Vec<Value>) {
    let mut systems = Vec::new();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < a.len() {
        if is_lti(&a[i]) {
            let spec = a
                .get(i + 1)
                .filter(|v| v.is_text())
                .and_then(|v| v.text().ok());
            if spec.is_some() {
                i += 1;
            }
            systems.push((a[i - usize::from(spec.is_some())].clone(), spec));
        } else {
            rest.push(a[i].clone());
        }
        i += 1;
    }
    (systems, rest)
}

fn plot_line(
    it: &mut Interp,
    x: Vec<f64>,
    y: Vec<f64>,
    spec: Option<&str>,
    stairs: bool,
) -> MResult<()> {
    let mut args = vec![Value::row(x), Value::row(y)];
    if let Some(s) = spec {
        args.push(Value::chars(s));
    }
    let f = if stairs { "stairs" } else { "plot" };
    it.call_named(f, args, 0)?;
    it.call_named("hold", vec![Value::chars("on")], 0)?;
    Ok(())
}

fn finish_axes(it: &mut Interp, title: &str, xl: &str, yl: &str) -> MResult<()> {
    it.call_named("hold", vec![Value::chars("off")], 0)?;
    let ax = it.graphics.current_axes();
    if ax.title.is_none() {
        ax.title = Some(title.into());
    }
    ax.xlabel.get_or_insert_with(|| xl.into());
    ax.ylabel.get_or_insert_with(|| yl.into());
    ax.grid_on = true;
    Ok(())
}

fn time_response(it: &mut Interp, a: Vec<Value>, nargout: usize, kind: Resp) -> R {
    let (systems, rest) = split_systems(&a);
    if systems.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (x0, tspec) = if kind == Resp::Initial {
        (
            rest.first()
                .map(|v| v.to_num())
                .transpose()?
                .map(|v| v.data)
                .unwrap_or_default(),
            rest.get(1),
        )
    } else {
        (Vec::new(), rest.first())
    };
    let models: Vec<Model> = systems
        .iter()
        .map(|(v, _)| obj_model(v))
        .collect::<MResult<_>>()?;
    if nargout > 0 {
        let m = &models[0];
        let t = time_grid(m, tspec)?;
        let resp = sampled_response(m, kind, &t, &x0);
        let p = m.state_space().c.nrows();
        // y is time x outputs (x inputs flattened into columns).
        let mut cols = Vec::new();
        for (y, _) in &resp {
            for o in 0..p {
                cols.extend(y.iter().map(|row| row[o]));
            }
        }
        let y = Value::mat(Array::new(t.len(), cols.len() / t.len().max(1), cols));
        let mut out = vec![y, Value::col(t.clone())];
        if nargout > 2 {
            let (_, xs) = &resp[0];
            let n = xs.first().map(|r| r.len()).unwrap_or(0);
            let mut data = Vec::new();
            for j in 0..n {
                data.extend(xs.iter().map(|r| r[j]));
            }
            out.push(Value::mat(Array::new(t.len(), n, data)));
        }
        return Ok(out);
    }
    // Plot all systems on a shared horizon.
    let tf = models.iter().map(default_tfinal).fold(0.0, f64::max);
    let title = match kind {
        Resp::Step => "Step Response",
        Resp::Impulse => "Impulse Response",
        Resp::Initial => "Response to Initial Conditions",
    };
    for ((_, spec), m) in systems.iter().zip(&models) {
        let t = match tspec {
            Some(v) => time_grid(m, Some(v))?,
            None => time_grid(
                m,
                Some(&Value::num(if m.ts > 0.0 {
                    (tf / m.ts).round() * m.ts
                } else {
                    tf
                })),
            )?,
        };
        for (y, _) in response_of(m, kind, &t, &x0) {
            let p = y.first().map(|r| r.len()).unwrap_or(1);
            for o in 0..p {
                plot_line(
                    it,
                    t.clone(),
                    y.iter().map(|r| r[o]).collect(),
                    spec.as_deref(),
                    m.ts != 0.0,
                )?;
            }
        }
    }
    finish_axes(it, title, "Time (seconds)", "Amplitude")?;
    none()
}

fn lsim(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let m = obj_model(&a[0])?;
    let u = a[1].to_num()?;
    let s = m.state_space();
    let mi = s.b.ncols();
    let t = match a.get(2) {
        Some(v) if !v.is_empty() => v.to_num()?.data,
        _ => {
            let ts = if m.ts > 0.0 { m.ts } else { 1.0 };
            (0..u.rows.max(u.cols)).map(|k| k as f64 * ts).collect()
        }
    };
    let n_t = t.len();
    // Input samples: one row per time (a row vector counts as one channel).
    let u_at = |k: usize| -> Vec<f64> {
        if u.rows == n_t {
            (0..mi).map(|j| *u.get(k, j.min(u.cols - 1))).collect()
        } else if u.cols == n_t {
            (0..mi).map(|j| *u.get(j.min(u.rows - 1), k)).collect()
        } else {
            vec![0.0; mi]
        }
    };
    if u.rows != n_t && u.cols != n_t {
        return Err(merr!(
            "The input signal must have one sample for each time point."
        ));
    }
    let x0 = a
        .get(3)
        .map(|v| v.to_num())
        .transpose()?
        .map(|v| v.data)
        .unwrap_or_default();
    let (y, xs) = simulate(&s, m.ts, &t, &u_at, &x0);
    let p = s.c.nrows();
    if nargout == 0 {
        let uu: Vec<f64> = (0..n_t).map(|k| u_at(k)[0]).collect();
        plot_line(it, t.clone(), uu, Some(":"), m.ts != 0.0)?;
        if let Some(ax) = it.graphics.current_figure().axes.last_mut() {
            if let Some(
                crate::graphics::Series::Line { style, .. }
                | crate::graphics::Series::Stairs { style, .. },
            ) = ax.series.last_mut()
            {
                style.color = Some("#A0A0A0".into());
            }
        }
        for o in 0..p {
            plot_line(
                it,
                t.clone(),
                y.iter().map(|r| r[o]).collect(),
                None,
                m.ts != 0.0,
            )?;
        }
        finish_axes(
            it,
            "Linear Simulation Results",
            "Time (seconds)",
            "Amplitude",
        )?;
        return none();
    }
    let mut cols = Vec::new();
    for o in 0..p {
        cols.extend(y.iter().map(|r| r[o]));
    }
    let mut out = vec![Value::mat(Array::new(n_t, p, cols)), Value::col(t)];
    if nargout > 2 {
        let n = s.a.nrows();
        let mut data = Vec::new();
        for j in 0..n {
            data.extend(xs.iter().map(|r| r[j]));
        }
        out.push(Value::mat(Array::new(n_t, n, data)));
    }
    Ok(out)
}

fn stepinfo(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (y, t, yfinal) = if is_lti(&a[0]) {
        let m = obj_model(&a[0])?;
        let tf = default_tfinal(&m) * 2.0;
        let t = time_grid(&m, Some(&Value::num(tf)))?;
        let y: Vec<f64> = response_of(&m, Resp::Step, &t, &[])[0]
            .0
            .iter()
            .map(|r| r[0])
            .collect();
        let (n, d) = m.rat()?;
        let s0 = if m.ts != 0.0 {
            c(1.0, 0.0)
        } else {
            c(0.0, 0.0)
        };
        let g = (peval(&n, s0) / peval(&d, s0)).re;
        (y, t, g)
    } else {
        let y = a[0].to_num()?.data;
        let t = match a.get(1) {
            Some(v) if !v.is_text() => v.to_num()?.data,
            _ => (0..y.len()).map(|k| k as f64).collect(),
        };
        let yf = match a.get(2) {
            Some(v) if !v.is_text() => v.scalar()?,
            _ => *y.last().unwrap_or(&0.0),
        };
        (y, t, yf)
    };
    let y0 = y.first().copied().unwrap_or(0.0);
    let span = yfinal - y0;
    let at = |frac: f64| -> f64 {
        let target = y0 + frac * span;
        for k in 1..y.len() {
            let (a1, b1) = (y[k - 1] - target, y[k] - target);
            if a1 == 0.0 {
                return t[k - 1];
            }
            if a1.signum() != b1.signum() {
                return t[k - 1] + (t[k] - t[k - 1]) * a1 / (a1 - b1);
            }
        }
        f64::NAN
    };
    let rise = at(0.9) - at(0.1);
    let tol = 0.02 * span.abs();
    let mut settle = 0.0;
    for k in (0..y.len()).rev() {
        if (y[k] - yfinal).abs() > tol {
            settle = if k + 1 < t.len() { t[k + 1] } else { t[k] };
            break;
        }
    }
    let (kpk, peak) = y.iter().enumerate().fold((0, 0.0f64), |(bk, bv), (k, v)| {
        if v.abs() > bv {
            (k, v.abs())
        } else {
            (bk, bv)
        }
    });
    let after = y
        .iter()
        .enumerate()
        .filter(|(k, _)| t[*k] >= at(0.9).min(t[t.len() - 1]));
    let (smin, smax) = after.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (_, v)| {
        (lo.min(*v), hi.max(*v))
    });
    let overshoot = if span != 0.0 {
        ((y.iter()
            .map(|v| (v - y0) / span)
            .fold(f64::NEG_INFINITY, f64::max)
            - 1.0)
            * 100.0)
            .max(0.0)
    } else {
        0.0
    };
    let undershoot = if span != 0.0 {
        (-y.iter()
            .map(|v| (v - y0) / span)
            .fold(f64::INFINITY, f64::min)
            * 100.0)
            .max(0.0)
    } else {
        0.0
    };
    one(Value::Struct(StructArr::scalar(vec![
        ("RiseTime".into(), Value::num(rise)),
        ("TransientTime".into(), Value::num(settle)),
        ("SettlingTime".into(), Value::num(settle)),
        ("SettlingMin".into(), Value::num(smin)),
        ("SettlingMax".into(), Value::num(smax)),
        ("Overshoot".into(), Value::num(overshoot)),
        ("Undershoot".into(), Value::num(undershoot)),
        ("Peak".into(), Value::num(peak)),
        ("PeakTime".into(), Value::num(t[kpk])),
    ])))
}

// ---------------------------------------------------------------- frequency responses

fn default_freqs(models: &[Model]) -> Vec<f64> {
    let mut lo = f64::INFINITY;
    let mut hi = 0.0f64;
    let mut nyq = f64::INFINITY;
    for m in models {
        let mut mags: Vec<f64> = Vec::new();
        let conv_pt = |z: C64| {
            if m.ts > 0.0 {
                (z.ln() / m.ts).norm()
            } else {
                z.norm()
            }
        };
        mags.extend(m.poles().into_iter().map(conv_pt));
        if let Ok(z) = m.zeros() {
            mags.extend(z.into_iter().map(conv_pt));
        }
        for x in mags.into_iter().filter(|x| *x > 1e-12 && x.is_finite()) {
            lo = lo.min(x);
            hi = hi.max(x);
        }
        if m.ts > 0.0 {
            nyq = nyq.min(std::f64::consts::PI / m.ts);
        }
    }
    if !lo.is_finite() {
        lo = 1.0;
        hi = 1.0;
    }
    let a = (lo.log10().floor() - 1.0).max(-8.0);
    let mut b = hi.log10().ceil() + 1.0;
    if nyq.is_finite() {
        b = b.min(nyq.log10());
    }
    let n = 400;
    (0..n)
        .map(|k| 10f64.powf(a + (b - a) * k as f64 / (n - 1) as f64))
        .collect()
}

/// Phase in degrees, unwrapped along frequency, starting in (-180, 180].
fn phase_deg(r: &[C64]) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::with_capacity(r.len());
    for z in r {
        let p = z.arg().to_degrees();
        match out.last() {
            None => out.push(p),
            Some(prev) => {
                let mut q = p;
                while q - prev > 180.0 {
                    q -= 360.0;
                }
                while q - prev < -180.0 {
                    q += 360.0;
                }
                out.push(q);
            }
        }
    }
    out
}

fn models_and_w(a: &[Value]) -> MResult<(Vec<(Value, Option<String>)>, Vec<f64>)> {
    let (systems, rest) = split_systems(a);
    if systems.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let w = match rest.first() {
        Some(Value::Cell(cl)) => {
            let lo = cl.data[0].scalar()?;
            let hi = cl.data[1].scalar()?;
            (0..400)
                .map(|k| 10f64.powf(lo.log10() + (hi.log10() - lo.log10()) * k as f64 / 399.0))
                .collect()
        }
        Some(v) => v.to_num()?.data,
        None => {
            let ms: Vec<Model> = systems
                .iter()
                .filter_map(|(v, _)| obj_model(v).ok())
                .collect();
            if ms.is_empty() {
                // frd data: use its own frequencies.
                match &systems[0].0 {
                    Value::Obj(o) => o.get("Frequency").unwrap().to_num()?.data,
                    _ => vec![],
                }
            } else {
                default_freqs(&ms)
            }
        }
    };
    Ok((systems, w))
}

fn bode(it: &mut Interp, a: Vec<Value>, nargout: usize, mag_only: bool) -> R {
    let (systems, w) = models_and_w(&a)?;
    if nargout > 0 {
        let r = response(&systems[0].0, &w)?;
        let mag: Vec<f64> = r.iter().map(|z| z.norm()).collect();
        let ph = phase_deg(&r);
        return Ok(vec![Value::col(mag), Value::col(ph), Value::col(w)]);
    }
    let fig_axes = |it: &mut Interp, k: usize| -> MResult<()> {
        if !mag_only {
            it.call_named(
                "subplot",
                vec![Value::num(2.0), Value::num(1.0), Value::num(k as f64)],
                0,
            )?;
        }
        Ok(())
    };
    let data: Vec<(Vec<C64>, Option<String>)> = systems
        .iter()
        .map(|(v, s)| response(v, &w).map(|r| (r, s.clone())))
        .collect::<MResult<_>>()?;
    fig_axes(it, 1)?;
    for (r, spec) in &data {
        let db: Vec<f64> = r.iter().map(|z| 20.0 * z.norm().log10()).collect();
        it.call_named("semilogx", line_args(&w, db, spec.as_deref()), 0)?;
        it.call_named("hold", vec![Value::chars("on")], 0)?;
    }
    finish_axes(
        it,
        "Bode Diagram",
        if mag_only { "Frequency (rad/s)" } else { "" },
        "Magnitude (dB)",
    )?;
    if !mag_only {
        fig_axes(it, 2)?;
        for (r, spec) in &data {
            it.call_named("semilogx", line_args(&w, phase_deg(r), spec.as_deref()), 0)?;
            it.call_named("hold", vec![Value::chars("on")], 0)?;
        }
        finish_axes(it, "", "Frequency (rad/s)", "Phase (deg)")?;
        it.graphics.current_axes().title = None;
    }
    none()
}

fn line_args(x: &[f64], y: Vec<f64>, spec: Option<&str>) -> Vec<Value> {
    let mut v = vec![Value::row(x.to_vec()), Value::row(y)];
    if let Some(s) = spec {
        v.push(Value::chars(s));
    }
    v
}

fn nyquist(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let (systems, w) = models_and_w(&a)?;
    if nargout > 0 {
        let r = response(&systems[0].0, &w)?;
        return Ok(vec![
            Value::col(r.iter().map(|z| z.re).collect()),
            Value::col(r.iter().map(|z| z.im).collect()),
            Value::col(w),
        ]);
    }
    for (v, spec) in &systems {
        let r = response(v, &w)?;
        let (re, im): (Vec<f64>, Vec<f64>) = r.iter().map(|z| (z.re, z.im)).unzip();
        it.call_named("plot", line_args(&re, im.clone(), spec.as_deref()), 0)?;
        it.call_named("hold", vec![Value::chars("on")], 0)?;
        let neg: Vec<f64> = im.iter().map(|x| -x).collect();
        it.call_named("plot", line_args(&re, neg, Some("--")), 0)?;
    }
    it.call_named(
        "plot",
        vec![Value::num(-1.0), Value::num(0.0), Value::chars("r+")],
        0,
    )?;
    finish_axes(it, "Nyquist Diagram", "Real Axis", "Imaginary Axis")?;
    none()
}

fn nichols(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let (systems, w) = models_and_w(&a)?;
    let r = response(&systems[0].0, &w)?;
    let mag: Vec<f64> = r.iter().map(|z| z.norm()).collect();
    let ph = phase_deg(&r);
    if nargout > 0 {
        return Ok(vec![Value::col(mag), Value::col(ph), Value::col(w)]);
    }
    let db: Vec<f64> = mag.iter().map(|m| 20.0 * m.log10()).collect();
    it.call_named("plot", line_args(&ph, db, None), 0)?;
    finish_axes(
        it,
        "Nichols Chart",
        "Open-Loop Phase (deg)",
        "Open-Loop Gain (dB)",
    )?;
    none()
}

/// Gain margin (absolute), phase margin (deg) and their crossover frequencies.
fn margins(v: &Value) -> MResult<(f64, f64, f64, f64)> {
    let m = obj_model(v)?;
    let mut w = default_freqs(std::slice::from_ref(&m));
    // Widen the grid so crossovers near the ends are found.
    let (lo, hi) = (w[0] / 100.0, w[w.len() - 1] * 100.0);
    let hi = if m.ts > 0.0 {
        hi.min(std::f64::consts::PI / m.ts)
    } else {
        hi
    };
    w = (0..4000)
        .map(|k| 10f64.powf(lo.log10() + (hi.log10() - lo.log10()) * k as f64 / 3999.0))
        .collect();
    let r: Vec<C64> = w.iter().map(|x| m.freq(*x)).collect::<MResult<_>>()?;
    let ph = phase_deg(&r);
    let mag: Vec<f64> = r.iter().map(|z| z.norm()).collect();
    let refine = |f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64| -> f64 {
        let fa0 = f(a);
        for _ in 0..80 {
            let mid = (a * b).sqrt();
            if (f(mid) > 0.0) == (fa0 > 0.0) {
                a = mid;
            } else {
                b = mid;
            }
        }
        (a * b).sqrt()
    };
    // Phase crossover: phase = -180 (mod 360); smallest gain margin wins.
    let mut gm = f64::INFINITY;
    let mut wcg = f64::NAN;
    for k in 1..w.len() {
        let (p0, p1) = (ph[k - 1], ph[k]);
        let n0 = ((p0 + 180.0) / 360.0).floor();
        let n1 = ((p1 + 180.0) / 360.0).floor();
        if n0 != n1 {
            let target = n0.max(n1) * 360.0 - 180.0;
            let g = |x: f64| -> f64 {
                let z = m.freq(x).unwrap_or(c(f64::NAN, 0.0));
                // Phase continued near the bracket.
                let mut p = z.arg().to_degrees();
                while p - p0 > 180.0 {
                    p -= 360.0;
                }
                while p - p0 < -180.0 {
                    p += 360.0;
                }
                p - target
            };
            let x = refine(&g, w[k - 1], w[k]);
            let gain = 1.0 / m.freq(x)?.norm();
            if gain < gm {
                gm = gain;
                wcg = x;
            }
        }
    }
    // Gain crossover: |G| = 1; smallest phase margin wins.
    let mut pm = f64::INFINITY;
    let mut wcp = f64::NAN;
    for k in 1..w.len() {
        if (mag[k - 1] - 1.0).signum() != (mag[k] - 1.0).signum() {
            let x = refine(
                &|x| m.freq(x).map(|z| z.norm() - 1.0).unwrap_or(f64::NAN),
                w[k - 1],
                w[k],
            );
            let z = m.freq(x)?;
            let mut p = z.arg().to_degrees();
            while p - ph[k - 1] > 180.0 {
                p -= 360.0;
            }
            while p - ph[k - 1] < -180.0 {
                p += 360.0;
            }
            let margin = p + 180.0 - 360.0 * ((p + 180.0) / 360.0).floor();
            let margin = if margin > 180.0 {
                margin - 360.0
            } else {
                margin
            };
            if margin.abs() < pm.abs() || !pm.is_finite() {
                pm = margin;
                wcp = x;
            }
        }
    }
    Ok((gm, pm, wcg, wcp))
}

/// Margins from sampled magnitude, phase (degrees) and frequency, by interpolation
/// in log frequency.
fn data_margins(mag: &[f64], ph: &[f64], w: &[f64]) -> (f64, f64, f64, f64) {
    let n = mag.len().min(ph.len()).min(w.len());
    let lerp = |k: usize, f: f64| -> f64 {
        10f64.powf(w[k - 1].log10() + f * (w[k].log10() - w[k - 1].log10()))
    };
    let (mut gm, mut wcg) = (f64::INFINITY, f64::NAN);
    for k in 1..n {
        let (p0, p1) = (ph[k - 1], ph[k]);
        let n0 = ((p0 + 180.0) / 360.0).floor();
        let n1 = ((p1 + 180.0) / 360.0).floor();
        if n0 != n1 && p1 != p0 {
            let target = n0.max(n1) * 360.0 - 180.0;
            let f = (target - p0) / (p1 - p0);
            let m = mag[k - 1] + f * (mag[k] - mag[k - 1]);
            if 1.0 / m < gm {
                gm = 1.0 / m;
                wcg = lerp(k, f);
            }
        }
    }
    let (mut pm, mut wcp) = (f64::INFINITY, f64::NAN);
    for k in 1..n {
        if (mag[k - 1] - 1.0).signum() != (mag[k] - 1.0).signum() {
            let (l0, l1) = (mag[k - 1].log10(), mag[k].log10());
            let f = -l0 / (l1 - l0);
            let p = ph[k - 1] + f * (ph[k] - ph[k - 1]);
            let mut margin = p + 180.0 - 360.0 * ((p + 180.0) / 360.0).floor();
            if margin > 180.0 {
                margin -= 360.0;
            }
            if margin.abs() < pm.abs() {
                pm = margin;
                wcp = lerp(k, f);
            }
        }
    }
    (gm, pm, wcg, wcp)
}

fn margin(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (gm, pm, wcg, wcp) = if a.len() >= 3 && !is_lti(&a[0]) {
        data_margins(
            &a[0].to_num()?.data,
            &a[1].to_num()?.data,
            &a[2].to_num()?.data,
        )
    } else {
        margins(&a[0])?
    };
    if nargout > 0 {
        return Ok(vec![
            Value::num(gm),
            Value::num(pm),
            Value::num(wcg),
            Value::num(wcp),
        ]);
    }
    bode(it, vec![a[0].clone()], 0, false)?;
    let gm_db = 20.0 * gm.log10();
    let title = format!(
        "Bode Diagram\nGm = {} dB (at {} rad/s) ,  Pm = {} deg (at {} rad/s)",
        display::trim_g(gm_db, 3),
        display::trim_g(wcg, 3),
        display::trim_g(pm, 3),
        display::trim_g(wcp, 3)
    );
    // The title belongs to the magnitude axes.
    let fig = it.graphics.current_figure();
    if let Some(ax) = fig.axes.iter_mut().find(|x| x.title.is_some()) {
        ax.title = Some(title);
    }
    none()
}

fn rlocus(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = obj_model(&a[0])?;
    let (n, d) = m.rat()?;
    let gains: Vec<f64> = match a.get(1) {
        Some(v) => v.to_num()?.data,
        None => {
            let mut g = vec![0.0];
            g.extend((0..=300).map(|k| 10f64.powf(-3.0 + 7.0 * k as f64 / 300.0)));
            g
        }
    };
    let order = d.len() - 1;
    let mut branches: Vec<Vec<C64>> = Vec::new();
    for &k in &gains {
        let p = padd(&d, &pscale(&n, k));
        let mut r = roots(&p);
        r.resize(order, c(f64::NAN, f64::NAN));
        // Follow branches: match each new root to the nearest previous one.
        if let Some(prev) = branches.last() {
            let mut ordered = vec![c(f64::NAN, f64::NAN); order];
            let mut used = vec![false; order];
            for (i, q) in prev.iter().enumerate() {
                let mut best = None;
                let mut bd = f64::INFINITY;
                for (j, z) in r.iter().enumerate() {
                    if !used[j] && (z - q).norm() < bd {
                        bd = (z - q).norm();
                        best = Some(j);
                    }
                }
                if let Some(j) = best {
                    used[j] = true;
                    ordered[i] = r[j];
                }
            }
            r = ordered;
        }
        branches.push(r);
    }
    if nargout > 0 {
        let mut data = Vec::new();
        for k in 0..gains.len() {
            data.extend(branches[k].iter().copied());
        }
        // order x numel(gains), one column per gain.
        return Ok(vec![
            Value::complex(Array::new(order, gains.len(), data)),
            Value::row(gains),
        ]);
    }
    for b in 0..order {
        let re: Vec<f64> = branches.iter().map(|r| r[b].re).collect();
        let im: Vec<f64> = branches.iter().map(|r| r[b].im).collect();
        plot_line(it, re, im, None, false)?;
    }
    let p = m.poles();
    let z = m.zeros()?;
    it.call_named(
        "plot",
        vec![
            Value::row(p.iter().map(|q| q.re).collect()),
            Value::row(p.iter().map(|q| q.im).collect()),
            Value::chars("kx"),
        ],
        0,
    )?;
    if !z.is_empty() {
        it.call_named(
            "plot",
            vec![
                Value::row(z.iter().map(|q| q.re).collect()),
                Value::row(z.iter().map(|q| q.im).collect()),
                Value::chars("ko"),
            ],
            0,
        )?;
    }
    finish_axes(
        it,
        "Root Locus",
        "Real Axis (seconds^{-1})",
        "Imaginary Axis (seconds^{-1})",
    )?;
    none()
}

fn pzmap(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = obj_model(&a[0])?;
    let p = m.poles();
    let z = m.zeros()?;
    if nargout > 0 {
        return Ok(vec![
            Value::complex(Array::col(p)),
            Value::complex(Array::col(z)),
        ]);
    }
    it.call_named(
        "plot",
        vec![
            Value::row(p.iter().map(|q| q.re).collect()),
            Value::row(p.iter().map(|q| q.im).collect()),
            Value::chars("x"),
        ],
        0,
    )?;
    it.call_named("hold", vec![Value::chars("on")], 0)?;
    if !z.is_empty() {
        it.call_named(
            "plot",
            vec![
                Value::row(z.iter().map(|q| q.re).collect()),
                Value::row(z.iter().map(|q| q.im).collect()),
                Value::chars("o"),
            ],
            0,
        )?;
    }
    finish_axes(
        it,
        "Pole-Zero Map",
        "Real Axis (seconds^{-1})",
        "Imaginary Axis (seconds^{-1})",
    )?;
    none()
}
