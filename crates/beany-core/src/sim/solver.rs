//! ODE solvers for the simulator: fixed-step Euler, Heun and RK4; adaptive
//! Dormand–Prince 5(4) and Bogacki–Shampine 3(2) with error control, and a
//! linearly implicit Rosenbrock 2(3) method for stiff systems.

use super::SimError;
use crate::interp::Interp;

pub trait System {
    fn nx(&self) -> usize;
    fn f(&mut self, it: &mut Interp, t: f64, x: &[f64]) -> Result<Vec<f64>, SimError>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    Ode1,
    Ode2,
    Ode4,
    Ode23,
    Ode45,
    Ode23s,
    Discrete,
}

impl Method {
    pub fn parse(s: &str) -> Option<Method> {
        Some(match s {
            "ode1" => Method::Ode1,
            "ode2" => Method::Ode2,
            "ode4" => Method::Ode4,
            "ode23" => Method::Ode23,
            "ode45" => Method::Ode45,
            // No BDF solver yet: stiff requests use the Rosenbrock method.
            "ode23s" | "ode15s" => Method::Ode23s,
            "discrete" => Method::Discrete,
            _ => return None,
        })
    }
    pub fn is_fixed(self) -> bool {
        matches!(self, Method::Ode1 | Method::Ode2 | Method::Ode4)
    }
}

pub fn initial_step(max_step: f64, span: f64) -> f64 {
    (span * 1e-4).min(max_step).max(1e-9)
}

fn axpy(x: &[f64], h: f64, terms: &[(f64, &[f64])]) -> Vec<f64> {
    let mut out = x.to_vec();
    for (c, k) in terms {
        if *c != 0.0 {
            for i in 0..out.len() {
                out[i] += h * c * k[i];
            }
        }
    }
    out
}

pub fn fixed_step<S: System>(
    m: Method,
    s: &mut S,
    it: &mut Interp,
    t: f64,
    x: &[f64],
    h: f64,
) -> Result<Vec<f64>, SimError> {
    if s.nx() == 0 {
        return Ok(x.to_vec());
    }
    Ok(match m {
        Method::Ode1 => {
            let k1 = s.f(it, t, x)?;
            axpy(x, h, &[(1.0, &k1)])
        }
        Method::Ode2 => {
            let k1 = s.f(it, t, x)?;
            let k2 = s.f(it, t + h, &axpy(x, h, &[(1.0, &k1)]))?;
            axpy(x, h, &[(0.5, &k1), (0.5, &k2)])
        }
        _ => {
            let k1 = s.f(it, t, x)?;
            let k2 = s.f(it, t + h / 2.0, &axpy(x, h, &[(0.5, &k1)]))?;
            let k3 = s.f(it, t + h / 2.0, &axpy(x, h, &[(0.5, &k2)]))?;
            let k4 = s.f(it, t + h, &axpy(x, h, &[(1.0, &k3)]))?;
            axpy(
                x,
                h,
                &[
                    (1.0 / 6.0, &k1),
                    (1.0 / 3.0, &k2),
                    (1.0 / 3.0, &k3),
                    (1.0 / 6.0, &k4),
                ],
            )
        }
    })
}

/// One embedded Runge–Kutta step: (higher-order solution, error estimate).
fn embedded<S: System>(
    m: Method,
    s: &mut S,
    it: &mut Interp,
    t: f64,
    x: &[f64],
    h: f64,
) -> Result<(Vec<f64>, Vec<f64>), SimError> {
    if m == Method::Ode23s {
        return rosenbrock(s, it, t, x, h);
    }
    if m == Method::Ode23 {
        let k1 = s.f(it, t, x)?;
        let k2 = s.f(it, t + h / 2.0, &axpy(x, h, &[(0.5, &k1)]))?;
        let k3 = s.f(it, t + 0.75 * h, &axpy(x, h, &[(0.75, &k2)]))?;
        let y = axpy(
            x,
            h,
            &[(2.0 / 9.0, &k1), (1.0 / 3.0, &k2), (4.0 / 9.0, &k3)],
        );
        let k4 = s.f(it, t + h, &y)?;
        let e: Vec<f64> = (0..x.len())
            .map(|i| {
                h * ((2.0 / 9.0 - 7.0 / 24.0) * k1[i]
                    + (1.0 / 3.0 - 0.25) * k2[i]
                    + (4.0 / 9.0 - 1.0 / 3.0) * k3[i]
                    - 0.125 * k4[i])
            })
            .collect();
        return Ok((y, e));
    }
    let k1 = s.f(it, t, x)?;
    let k2 = s.f(it, t + h / 5.0, &axpy(x, h, &[(1.0 / 5.0, &k1)]))?;
    let k3 = s.f(
        it,
        t + 3.0 * h / 10.0,
        &axpy(x, h, &[(3.0 / 40.0, &k1), (9.0 / 40.0, &k2)]),
    )?;
    let k4 = s.f(
        it,
        t + 4.0 * h / 5.0,
        &axpy(
            x,
            h,
            &[(44.0 / 45.0, &k1), (-56.0 / 15.0, &k2), (32.0 / 9.0, &k3)],
        ),
    )?;
    let k5 = s.f(
        it,
        t + 8.0 * h / 9.0,
        &axpy(
            x,
            h,
            &[
                (19372.0 / 6561.0, &k1),
                (-25360.0 / 2187.0, &k2),
                (64448.0 / 6561.0, &k3),
                (-212.0 / 729.0, &k4),
            ],
        ),
    )?;
    let k6 = s.f(
        it,
        t + h,
        &axpy(
            x,
            h,
            &[
                (9017.0 / 3168.0, &k1),
                (-355.0 / 33.0, &k2),
                (46732.0 / 5247.0, &k3),
                (49.0 / 176.0, &k4),
                (-5103.0 / 18656.0, &k5),
            ],
        ),
    )?;
    let b = [
        35.0 / 384.0,
        0.0,
        500.0 / 1113.0,
        125.0 / 192.0,
        -2187.0 / 6784.0,
        11.0 / 84.0,
    ];
    let y = axpy(
        x,
        h,
        &[
            (b[0], &k1),
            (b[2], &k3),
            (b[3], &k4),
            (b[4], &k5),
            (b[5], &k6),
        ],
    );
    let k7 = s.f(it, t + h, &y)?;
    let bs = [
        5179.0 / 57600.0,
        0.0,
        7571.0 / 16695.0,
        393.0 / 640.0,
        -92097.0 / 339200.0,
        187.0 / 2100.0,
        1.0 / 40.0,
    ];
    let ks = [&k1, &k2, &k3, &k4, &k5, &k6, &k7];
    let bfull = [b[0], b[1], b[2], b[3], b[4], b[5], 0.0];
    let e: Vec<f64> = (0..x.len())
        .map(|i| h * (0..7).map(|j| (bfull[j] - bs[j]) * ks[j][i]).sum::<f64>())
        .collect();
    Ok((y, e))
}

/// One Rosenbrock 2(3) step (the Shampine–Reichelt pair) with a
/// finite-difference Jacobian: (second-order solution, error estimate).
fn rosenbrock<S: System>(
    s: &mut S,
    it: &mut Interp,
    t: f64,
    x: &[f64],
    h: f64,
) -> Result<(Vec<f64>, Vec<f64>), SimError> {
    use nalgebra::{DMatrix, DVector};
    let n = x.len();
    let d = 1.0 / (2.0 + std::f64::consts::SQRT_2);
    let e32 = 6.0 + std::f64::consts::SQRT_2;
    let f0 = s.f(it, t, x)?;
    let mut jac = DMatrix::<f64>::zeros(n, n);
    let mut xp = x.to_vec();
    for j in 0..n {
        let del = f64::EPSILON.sqrt() * x[j].abs().max(1.0);
        xp[j] = x[j] + del;
        let fj = s.f(it, t, &xp)?;
        xp[j] = x[j];
        for i in 0..n {
            jac[(i, j)] = (fj[i] - f0[i]) / del;
        }
    }
    let dt = f64::EPSILON.sqrt() * t.abs().max(1.0);
    let ft = s.f(it, t + dt, x)?;
    let tder: Vec<f64> = (0..n).map(|i| (ft[i] - f0[i]) / dt).collect();
    let w = DMatrix::<f64>::identity(n, n) - jac * (h * d);
    let lu = w.lu();
    let solve = |rhs: Vec<f64>| -> Result<Vec<f64>, SimError> {
        lu.solve(&DVector::from_vec(rhs))
            .map(|v| v.as_slice().to_vec())
            .ok_or_else(|| SimError {
                message: format!(
                    "The stiff solver met a singular iteration matrix at t = {}.",
                    t
                ),
                block: None,
            })
    };
    let k1 = solve((0..n).map(|i| f0[i] + h * d * tder[i]).collect())?;
    let f1 = s.f(it, t + 0.5 * h, &axpy(x, h, &[(0.5, &k1)]))?;
    let k2: Vec<f64> = solve((0..n).map(|i| f1[i] - k1[i]).collect())?
        .iter()
        .zip(&k1)
        .map(|(a, b)| a + b)
        .collect();
    let y = axpy(x, h, &[(1.0, &k2)]);
    let f2 = s.f(it, t + h, &y)?;
    let k3 = solve(
        (0..n)
            .map(|i| f2[i] - e32 * (k2[i] - f1[i]) - 2.0 * (k1[i] - f0[i]) + h * d * tder[i])
            .collect(),
    )?;
    let e = (0..n)
        .map(|i| h / 6.0 * (k1[i] - 2.0 * k2[i] + k3[i]))
        .collect();
    Ok((y, e))
}

/// Adaptive step from t. Returns (x(t+h), h used, suggested next h).
#[allow(clippy::too_many_arguments)]
pub fn adaptive_step<S: System>(
    m: Method,
    s: &mut S,
    it: &mut Interp,
    t: f64,
    x: &[f64],
    h0: f64,
    h_max: f64,
    rtol: f64,
    atol: f64,
) -> Result<(Vec<f64>, f64, f64), SimError> {
    if s.nx() == 0 {
        return Ok((x.to_vec(), h_max, h_max));
    }
    let order = if m == Method::Ode45 { 5.0 } else { 3.0 };
    let mut h = h0.max(1e-14).min(h_max);
    for _ in 0..60 {
        let (y, e) = embedded(m, s, it, t, x, h)?;
        let err = (0..x.len())
            .map(|i| e[i].abs() / (atol + rtol * x[i].abs().max(y[i].abs())))
            .fold(0.0f64, f64::max);
        if !err.is_finite() {
            h *= 0.25;
            continue;
        }
        if err <= 1.0 || h <= 1e-12 * t.abs().max(1.0) {
            let grow = if err == 0.0 {
                5.0
            } else {
                (0.9 * err.powf(-1.0 / order)).clamp(0.2, 5.0)
            };
            return Ok((y, h, (h * grow).min(h_max.max(h))));
        }
        h *= (0.9 * err.powf(-1.0 / order)).clamp(0.1, 0.9);
    }
    Err(SimError { message: format!("The solver could not reach the requested accuracy at t = {}. Try a smaller max step or looser tolerances.", t), block: None })
}

/// Advance exactly h without error control (used to locate zero crossings).
pub fn step_to<S: System>(
    m: Method,
    s: &mut S,
    it: &mut Interp,
    t: f64,
    x: &[f64],
    h: f64,
) -> Result<Vec<f64>, SimError> {
    match m {
        Method::Discrete => Ok(x.to_vec()),
        Method::Ode23 | Method::Ode45 | Method::Ode23s => Ok(embedded(m, s, it, t, x, h)?.0),
        _ => fixed_step(m, s, it, t, x, h),
    }
}
