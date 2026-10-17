//! `ode45`, `ode23` and `odeset`, built on the simulator's adaptive solvers.

use super::solver::{self, Method, System};
use super::SimError;
use crate::error::{MError, MResult};
use crate::graphics::{LineStyle, Series, DEFAULT_COLORS};
use crate::interp::Interp;
use crate::merr;
use crate::value::{Array, StructArr, Value};

struct FnSystem {
    f: Value,
    n: usize,
}

impl System for FnSystem {
    fn nx(&self) -> usize {
        self.n
    }
    fn f(&mut self, it: &mut Interp, t: f64, x: &[f64]) -> Result<Vec<f64>, SimError> {
        let v = it
            .call1(&self.f, vec![Value::num(t), Value::col(x.to_vec())])
            .map_err(|e| SimError {
                message: e.report(),
                block: None,
            })?;
        let d = v
            .to_num()
            .map_err(|e| SimError {
                message: e.message,
                block: None,
            })?
            .data;
        if d.len() != self.n {
            return Err(SimError {
                message: format!(
                    "The ODE function returned {} values, but the initial condition has {}.",
                    d.len(),
                    self.n
                ),
                block: None,
            });
        }
        Ok(d)
    }
}

struct Opts {
    rtol: f64,
    atol: f64,
    max_step: Option<f64>,
    initial_step: Option<f64>,
}

fn options(v: Option<&Value>) -> MResult<Opts> {
    let mut o = Opts {
        rtol: 1e-3,
        atol: 1e-6,
        max_step: None,
        initial_step: None,
    };
    if let Some(Value::Struct(s)) = v {
        let get = |k: &str| {
            s.get(0, k)
                .filter(|v| !v.is_empty())
                .map(|v| v.scalar())
                .transpose()
        };
        o.rtol = get("RelTol")?.unwrap_or(o.rtol);
        o.atol = get("AbsTol")?.unwrap_or(o.atol);
        o.max_step = get("MaxStep")?;
        o.initial_step = get("InitialStep")?;
    }
    Ok(o)
}

/// Cubic Hermite interpolation between two solution points.
fn hermite(t0: f64, y0: &[f64], f0: &[f64], t1: f64, y1: &[f64], f1: &[f64], t: f64) -> Vec<f64> {
    let h = t1 - t0;
    let s = (t - t0) / h;
    let (h00, h10, h01, h11) = (
        2.0 * s.powi(3) - 3.0 * s * s + 1.0,
        s.powi(3) - 2.0 * s * s + s,
        -2.0 * s.powi(3) + 3.0 * s * s,
        s.powi(3) - s * s,
    );
    (0..y0.len())
        .map(|i| h00 * y0[i] + h10 * h * f0[i] + h01 * y1[i] + h11 * h * f1[i])
        .collect()
}

fn solve(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    m: Method,
    name: &str,
) -> MResult<Vec<Value>> {
    if a.len() < 3 {
        return Err(merr!(
            "Usage: [t, y] = {}(@(t, y) f, [t0 tf], y0[, options])",
            name
        ));
    }
    let tspan = a[1].to_num()?.data;
    if tspan.len() < 2 {
        return Err(merr!(
            "The time span must have at least two values [t0 tf]."
        ));
    }
    let y0 = a[2].to_num()?.data;
    let o = options(a.get(3))?;
    let (t0, tf) = (tspan[0], *tspan.last().unwrap());
    let dir = if tf >= t0 { 1.0 } else { -1.0 };
    if dir < 0.0 {
        return Err(merr!("Integrating backwards in time is not supported yet."));
    }
    let span = tf - t0;
    let max_step = o.max_step.unwrap_or(span / 10.0).max(1e-12);
    let mut sys = FnSystem {
        f: a[0].clone(),
        n: y0.len(),
    };
    let mut t = t0;
    let mut y = y0.clone();
    let mut f_here = sys.f(it, t, &y).map_err(|e| MError::new(e.message))?;
    let mut ts = vec![t];
    let mut ys = vec![y.clone()];
    let mut fs = vec![f_here.clone()];
    let mut h = o
        .initial_step
        .unwrap_or(solver::initial_step(max_step, span));
    let interrupt = it.interrupt.clone();
    while t < tf - 1e-12 * tf.abs().max(1.0) {
        if interrupt.load(std::sync::atomic::Ordering::Relaxed) {
            interrupt.store(false, std::sync::atomic::Ordering::Relaxed);
            return Err(MError::interrupted());
        }
        let limit = (tf - t).min(max_step);
        let (y1, used, next) =
            solver::adaptive_step(m, &mut sys, it, t, &y, h.min(limit), limit, o.rtol, o.atol)
                .map_err(|e| MError::new(e.message))?;
        t += used;
        y = y1;
        f_here = sys.f(it, t, &y).map_err(|e| MError::new(e.message))?;
        ts.push(t);
        ys.push(y.clone());
        fs.push(f_here.clone());
        h = next;
    }
    // Output points: the requested times, or each step refined 4x like the reference solver.
    let (tout, yout): (Vec<f64>, Vec<Vec<f64>>) = if tspan.len() > 2 {
        let mut k = 0;
        let mut out = Vec::new();
        for &tq in &tspan {
            while k + 2 < ts.len() && ts[k + 1] < tq {
                k += 1;
            }
            out.push(if (tq - ts[k]).abs() < 1e-14 {
                ys[k].clone()
            } else {
                hermite(ts[k], &ys[k], &fs[k], ts[k + 1], &ys[k + 1], &fs[k + 1], tq)
            });
        }
        (tspan.clone(), out)
    } else {
        let refine = if m == Method::Ode45 { 4 } else { 1 };
        let mut to = vec![ts[0]];
        let mut yo = vec![ys[0].clone()];
        for k in 0..ts.len() - 1 {
            for r in 1..=refine {
                let tq = ts[k] + (ts[k + 1] - ts[k]) * r as f64 / refine as f64;
                to.push(tq);
                yo.push(if r == refine {
                    ys[k + 1].clone()
                } else {
                    hermite(ts[k], &ys[k], &fs[k], ts[k + 1], &ys[k + 1], &fs[k + 1], tq)
                });
            }
        }
        (to, yo)
    };
    let n = y0.len();
    let mut ymat = Array::filled(tout.len(), n, 0.0);
    for (r, row) in yout.iter().enumerate() {
        for c in 0..n {
            ymat.set(r, c, row[c]);
        }
    }
    if nargout == 0 {
        let ax = it.graphics.plot_axes();
        for c in 0..n {
            let col: Vec<f64> = (0..tout.len()).map(|r| *ymat.get(r, c)).collect();
            let color = DEFAULT_COLORS[ax.color_index % DEFAULT_COLORS.len()].to_string();
            ax.color_index += 1;
            ax.series.push(Series::Line {
                x: tout.clone(),
                y: col,
                z: None,
                style: LineStyle {
                    color: Some(color),
                    marker: Some("o".into()),
                    marker_size: Some(4.0),
                    line: Some("-".into()),
                    ..Default::default()
                },
                name: None,
            });
        }
        return Ok(vec![]);
    }
    if nargout == 1 {
        return Ok(vec![Value::Struct(StructArr::scalar(vec![
            ("solver".into(), Value::chars(name)),
            ("x".into(), Value::row(tout)),
            ("y".into(), Value::mat(ymat.transpose())),
        ]))]);
    }
    Ok(vec![Value::col(tout), Value::mat(ymat)])
}

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("ode45", |it, a, n| solve(it, a, n, Method::Ode45, "ode45"));
    b.insert("ode23", |it, a, n| solve(it, a, n, Method::Ode23, "ode23"));
    b.insert("odeset", |_, a, _| {
        let mut fields = Vec::new();
        for pair in a.chunks(2) {
            let k = pair[0].text()?;
            let v = pair
                .get(1)
                .cloned()
                .ok_or_else(|| merr!("odeset expects name/value pairs."))?;
            let canon = match k.to_lowercase().as_str() {
                "reltol" => "RelTol",
                "abstol" => "AbsTol",
                "maxstep" => "MaxStep",
                "initialstep" => "InitialStep",
                "events" | "mass" | "jacobian" | "outputfcn" | "refine" | "stats"
                | "nonnegative" => {
                    return Err(merr!("odeset option '{}' is not supported yet.", k));
                }
                _ => return Err(merr!("Unknown odeset option '{}'.", k)),
            };
            fields.push((canon.to_string(), v));
        }
        Ok(vec![Value::Struct(StructArr::scalar(fields))])
    });
}
