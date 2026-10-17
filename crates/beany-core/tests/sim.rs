//! Reference-model suite for the block-diagram simulator (ROADMAP M1 exit criteria).
//! Each model in examples/models is run and compared with an analytic or independently
//! computed answer.

use std::path::PathBuf;

use beany_core::value::Value;
use beany_core::Interp;

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/models")
}

fn run(model: &str) -> Interp {
    let mut it = Interp::new();
    it.cwd = models_dir();
    if let Err(e) = it.eval_string(&format!("out = sim('{}');", model)) {
        panic!("{} failed: {}", model, e.report());
    }
    it
}

fn field(it: &Interp, name: &str) -> Vec<f64> {
    let Some(Value::Struct(s)) = it.get_var("out") else {
        panic!("no out struct")
    };
    s.get(0, name)
        .unwrap_or_else(|| panic!("out has no field {}", name))
        .to_num()
        .unwrap()
        .data
}

/// Value of a logged signal at time `t` (linear interpolation between logged steps).
fn at(tout: &[f64], y: &[f64], t: f64) -> f64 {
    let k = tout
        .iter()
        .position(|v| *v >= t)
        .expect("time beyond the simulation");
    if k == 0 || (tout[k] - t).abs() < 1e-12 {
        return y[k];
    }
    let f = (t - tout[k - 1]) / (tout[k] - tout[k - 1]);
    y[k - 1] + f * (y[k] - y[k - 1])
}

#[test]
fn first_order_lag_matches_exponential() {
    let it = run("first_order");
    let (t, y) = (field(&it, "tout"), field(&it, "y"));
    assert!((t.last().unwrap() - 6.0).abs() < 1e-9);
    for &tt in &[0.5, 1.0, 1.5, 2.0, 3.0, 6.0] {
        let exact = if tt <= 1.0 {
            0.0
        } else {
            1.0 - (-(tt - 1.0f64)).exp()
        };
        let got = at(&t, &y, tt);
        assert!(
            (got - exact).abs() < 2e-3,
            "t={} got {} exact {}",
            tt,
            got,
            exact
        );
    }
}

#[test]
fn mass_spring_damper_matches_analytic() {
    let it = run("mass_spring_damper");
    let (t, x) = (field(&it, "tout"), field(&it, "x"));
    let (m, c, k): (f64, f64, f64) = (1.0, 0.5, 2.0);
    let wn = (k / m).sqrt();
    let z = c / (2.0 * (k * m).sqrt());
    let wd = wn * (1.0 - z * z).sqrt();
    for &tt in &[1.0, 2.5, 5.0, 10.0, 20.0] {
        let exact = (1.0 / k)
            * (1.0
                - (-z * wn * tt).exp()
                    * ((wd * tt).cos() + z / (1.0 - z * z).sqrt() * (wd * tt).sin()));
        let got = at(&t, &x, tt);
        assert!(
            (got - exact).abs() < 1e-4,
            "t={} got {} exact {}",
            tt,
            got,
            exact
        );
    }
}

#[test]
fn pid_loop_matches_closed_loop_transfer_function() {
    let mut it = run("pid_control");
    let (t, y) = (field(&it, "tout"), field(&it, "speed"));
    // Closed loop (2s + 1)/(s^2 + 3s + 1), computed independently with tf/step.
    it.eval_string("ref = step(tf([2 1], [1 3 1]), [0.5 1 2 5 10]);")
        .unwrap();
    let r = it.get_var("ref").unwrap().to_num().unwrap().data;
    for (i, &tt) in [0.5, 1.0, 2.0, 5.0, 10.0].iter().enumerate() {
        let got = at(&t, &y, tt);
        assert!(
            (got - r[i]).abs() < 1e-3,
            "t={} got {} ref {}",
            tt,
            got,
            r[i]
        );
    }
}

#[test]
fn discrete_low_pass_matches_difference_equation() {
    let it = run("discrete_filter");
    let (t, y) = (field(&it, "tout"), field(&it, "yf"));
    for k in 0..=20 {
        let tt = k as f64 * 0.1;
        let i = t
            .iter()
            .position(|v| (v - tt).abs() < 1e-9)
            .unwrap_or_else(|| panic!("no sample at t={}", tt));
        let exact = 1.0 - 0.8f64.powi(k);
        assert!(
            (y[i] - exact).abs() < 1e-12,
            "k={} got {} exact {}",
            k,
            y[i],
            exact
        );
    }
}

#[test]
fn bouncing_ball_reverses_at_impacts() {
    let it = run("bouncing_ball");
    let (t, h, v) = (field(&it, "tout"), field(&it, "h"), field(&it, "v"));
    let g = 9.81f64;
    let t1 = (2.0 * 10.0 / g).sqrt();
    let v1 = g * t1;
    let t2 = t1 + 2.0 * 0.8 * v1 / g;
    // Velocity jumps from negative to positive at each impact.
    let jumps: Vec<f64> = (1..v.len())
        .filter(|&i| v[i - 1] < 0.0 && v[i] > 0.0)
        .map(|i| t[i])
        .collect();
    assert!(
        jumps.len() >= 2,
        "expected at least two bounces, found {:?}",
        jumps
    );
    assert!(
        (jumps[0] - t1).abs() < 1e-3,
        "first impact at {} expected {}",
        jumps[0],
        t1
    );
    assert!(
        (jumps[1] - t2).abs() < 2e-3,
        "second impact at {} expected {}",
        jumps[1],
        t2
    );
    assert!(h.iter().all(|x| *x > -1e-6), "ball went through the floor");
}

/// Independent fine-step RK4 solution of the Van der Pol equation (mu = 1).
fn vdp_reference(t_end: f64) -> f64 {
    let f = |x: [f64; 2]| [x[1], (1.0 - x[0] * x[0]) * x[1] - x[0]];
    let mut x = [2.0, 0.0];
    let h = 1e-4;
    let n = (t_end / h).round() as usize;
    for _ in 0..n {
        let k1 = f(x);
        let k2 = f([x[0] + h / 2.0 * k1[0], x[1] + h / 2.0 * k1[1]]);
        let k3 = f([x[0] + h / 2.0 * k2[0], x[1] + h / 2.0 * k2[1]]);
        let k4 = f([x[0] + h * k3[0], x[1] + h * k3[1]]);
        for i in 0..2 {
            x[i] += h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
        }
    }
    x[0]
}

#[test]
fn van_der_pol_matches_reference_integration() {
    let it = run("van_der_pol");
    let (t, x1) = (field(&it, "tout"), field(&it, "x1"));
    for &tt in &[2.0, 5.0, 10.0] {
        let exact = vdp_reference(tt);
        let got = at(&t, &x1, tt);
        assert!(
            (got - exact).abs() < 5e-3,
            "t={} got {} ref {}",
            tt,
            got,
            exact
        );
    }
}

#[test]
fn user_function_block_calls_m_file() {
    let it = run("user_function");
    let (t, s) = (field(&it, "tout"), field(&it, "s"));
    for (i, tt) in t.iter().enumerate().step_by(37) {
        assert!((s[i] - tt.sin().powi(2)).abs() < 1e-9);
    }
}

#[test]
fn subsystem_flattens_to_same_result_as_flat_pid() {
    let it = run("subsystem_demo");
    let (t, y) = (field(&it, "tout"), field(&it, "y"));
    // Kp = 2, Ki = 1 around 1/(s+1): same closed loop as pid_control.
    let mut it2 = Interp::new();
    it2.eval_string("ref = step(tf([2 1], [1 3 1]), [1 4 10]);")
        .unwrap();
    let r = it2.get_var("ref").unwrap().to_num().unwrap().data;
    for (i, &tt) in [1.0, 4.0, 10.0].iter().enumerate() {
        assert!((at(&t, &y, tt) - r[i]).abs() < 1e-3);
    }
}

#[test]
fn algebraic_loop_is_reported() {
    let mut it = Interp::new();
    it.cwd = models_dir();
    let e = it.eval_string("sim('algebraic_loop')").unwrap_err();
    assert!(e.message.contains("Algebraic loop"), "{}", e.message);
    assert!(
        e.message.contains("Feedback gain") && e.message.contains("Sum"),
        "{}",
        e.message
    );
}

#[test]
fn get_param_and_sim_options() {
    let mut it = Interp::new();
    it.cwd = models_dir();
    it.eval_string(
        "k = get_param('first_order/Plant', 'den'); o = sim('first_order', 'StopTime', '2');",
    )
    .unwrap();
    assert_eq!(it.get_var("k").unwrap().text().unwrap(), "[1 1]");
    let Some(Value::Struct(s)) = it.get_var("o") else {
        panic!()
    };
    let tout = s.get(0, "tout").unwrap().to_num().unwrap().data;
    assert!((tout.last().unwrap() - 2.0).abs() < 1e-9);
}

#[test]
fn every_library_block_runs() {
    // Each block type wired with default parameters into a small model must simulate.
    let lib = beany_core::sim::blocks::library();
    for def in lib.iter().filter(|d| {
        !matches!(
            d.kind,
            "Subsystem" | "Inport" | "StopSimulation" | "UserFunction" | "FromWorkspace"
        )
    }) {
        let (ni, no) = beany_core::sim::blocks::ports_for(def.kind, &Default::default(), None);
        let mut blocks = vec![format!(r#"{{"id":"b","type":"{}"}}"#, def.kind)];
        let mut lines = Vec::new();
        for k in 0..ni {
            blocks.push(format!(r#"{{"id":"s{}","type":"SineWave"}}"#, k));
            lines.push(format!(
                r#"{{"src":{{"block":"s{}","port":1}},"dst":{{"block":"b","port":{}}}}}"#,
                k,
                k + 1
            ));
        }
        for k in 0..no {
            blocks.push(format!(r#"{{"id":"t{}","type":"Terminator"}}"#, k));
            lines.push(format!(
                r#"{{"src":{{"block":"b","port":{}}},"dst":{{"block":"t{}","port":1}}}}"#,
                k + 1,
                k
            ));
        }
        let json = format!(
            r#"{{"solver":{{"stop_time":"2"}},"blocks":[{}],"lines":[{}]}}"#,
            blocks.join(","),
            lines.join(",")
        );
        let model = beany_core::sim::Model::parse(&json).unwrap();
        let mut it = Interp::new();
        if let Err(e) = beany_core::sim::run::run(&mut it, &model, def.kind) {
            panic!("{} failed: {}", def.kind, e.0.report());
        }
    }
}
