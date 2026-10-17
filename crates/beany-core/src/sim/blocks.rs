//! Block library: definitions (for the editor and dialogs) and runtime behavior.

use std::collections::{BTreeMap, VecDeque};

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use serde::Serialize;

use crate::interp::Interp;
use crate::value::{Array, Value};

pub type SimResult<T> = Result<T, String>;

/// When a block computes a new output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    /// Every solver evaluation (continuous or inherited).
    Continuous,
    /// Only at t = offset + k * period; output held in between.
    Discrete { period: f64, offset: f64 },
    /// Never changes after the start.
    Constant,
}

/// What a sink records.
#[derive(Debug, Clone, PartialEq)]
pub enum LogKind {
    /// `separate`: one axes per input instead of all signals on one axes.
    Scope {
        separate: bool,
    },
    ToWorkspace(String),
    Outport(usize),
    Display,
}

pub struct Ctx<'a> {
    pub interp: &'a mut Interp,
    /// True for a major time step (outputs are final, logging happens).
    pub major: bool,
}

pub trait Block: Send {
    fn ports(&self) -> (usize, usize);
    /// Whether the output depends directly on this input at the same instant.
    fn feedthrough(&self, _input: usize) -> bool {
        true
    }
    fn sample(&self) -> Sample {
        Sample::Continuous
    }
    fn nstates(&self) -> usize {
        0
    }
    fn init(&mut self, _x: &mut [f64]) {}
    fn output(
        &mut self,
        ctx: &mut Ctx,
        t: f64,
        x: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()>;
    fn derivative(&self, _t: f64, _x: &[f64], _u: &[Vec<f64>], _dx: &mut [f64]) {}
    /// Discrete update at a sample hit, after outputs are computed.
    fn update(&mut self, _t: f64, _x: &mut [f64], _u: &[Vec<f64>]) {}
    /// Signals whose sign changes mark events the solver must locate exactly.
    fn zero_crossings(&self, _t: f64, _x: &[f64], _u: &[Vec<f64>]) -> Vec<f64> {
        Vec::new()
    }
    /// Called at a located zero crossing: index into `zero_crossings`, and whether the
    /// signal went from negative to non-negative.
    fn event(&mut self, _t: f64, _x: &mut [f64], _u: &[Vec<f64>], _which: usize, _rising: bool) {}
    /// Clamp continuous states after each step (integrator limits).
    fn project(&self, _x: &mut [f64]) {}
    fn log(&self) -> Option<LogKind> {
        None
    }
    /// Stop Simulation block.
    fn stops(&self, _u: &[Vec<f64>]) -> bool {
        false
    }
}

// ---------------------------------------------------------------- definitions

#[derive(Debug, Clone, Serialize)]
pub struct ParamDef {
    pub name: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    /// "expr" (evaluated in the workspace), "text", or "choice:a|b|c".
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockDef {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub params: Vec<ParamDef>,
}

const fn p(name: &'static str, label: &'static str, default: &'static str) -> ParamDef {
    ParamDef {
        name,
        label,
        default,
        kind: "expr",
    }
}
const fn t(name: &'static str, label: &'static str, default: &'static str) -> ParamDef {
    ParamDef {
        name,
        label,
        default,
        kind: "text",
    }
}
const fn c(
    name: &'static str,
    label: &'static str,
    default: &'static str,
    kind: &'static str,
) -> ParamDef {
    ParamDef {
        name,
        label,
        default,
        kind,
    }
}

pub fn library() -> Vec<BlockDef> {
    let d = |kind, category, description, params: Vec<ParamDef>| BlockDef {
        kind,
        category,
        description,
        params,
    };
    vec![
        // Sources
        d(
            "Constant",
            "Sources",
            "Outputs a constant value.",
            vec![p("value", "Constant value", "1")],
        ),
        d(
            "Step",
            "Sources",
            "Steps from an initial to a final value at the step time.",
            vec![
                p("time", "Step time", "1"),
                p("initial", "Initial value", "0"),
                p("final", "Final value", "1"),
            ],
        ),
        d(
            "Ramp",
            "Sources",
            "Ramp starting at the start time.",
            vec![
                p("slope", "Slope", "1"),
                p("start", "Start time", "0"),
                p("initial", "Initial output", "0"),
            ],
        ),
        d(
            "SineWave",
            "Sources",
            "A*sin(w*t + phase) + bias.",
            vec![
                p("amplitude", "Amplitude", "1"),
                p("bias", "Bias", "0"),
                p("frequency", "Frequency (rad/s)", "1"),
                p("phase", "Phase (rad)", "0"),
            ],
        ),
        d(
            "PulseGenerator",
            "Sources",
            "Rectangular pulses.",
            vec![
                p("amplitude", "Amplitude", "1"),
                p("period", "Period (s)", "1"),
                p("width", "Pulse width (% of period)", "50"),
                p("delay", "Phase delay (s)", "0"),
            ],
        ),
        d(
            "SignalGenerator",
            "Sources",
            "Sine, square or sawtooth wave.",
            vec![
                c("wave", "Wave form", "sine", "choice:sine|square|sawtooth"),
                p("amplitude", "Amplitude", "1"),
                p("frequency", "Frequency (Hz)", "1"),
            ],
        ),
        d("Clock", "Sources", "Outputs the simulation time.", vec![]),
        d(
            "FromWorkspace",
            "Sources",
            "Interpolates a [time, values] matrix from the workspace.",
            vec![p("data", "Data [t, y1, y2, ...]", "[0 0; 10 1]")],
        ),
        d(
            "RandomNumber",
            "Sources",
            "Normally distributed random numbers at a fixed sample time.",
            vec![
                p("mean", "Mean", "0"),
                p("variance", "Variance", "1"),
                p("seed", "Seed", "0"),
                p("ts", "Sample time (period or [period offset])", "0.1"),
            ],
        ),
        // Sinks
        d(
            "Scope",
            "Sinks",
            "Plots its inputs against time.",
            vec![
                p("inputs", "Number of inputs", "1"),
                c("layout", "Axes", "shared", "choice:shared|one per input"),
            ],
        ),
        d(
            "ToWorkspace",
            "Sinks",
            "Saves its input to out.<name>.",
            vec![t("variable", "Variable name", "simout")],
        ),
        d(
            "Display",
            "Sinks",
            "Shows the current value on the block.",
            vec![],
        ),
        d(
            "Outport",
            "Sinks",
            "Subsystem output, or a logged model output (out.yout).",
            vec![p("port", "Port number", "1")],
        ),
        d("Terminator", "Sinks", "Ends an unused signal.", vec![]),
        d(
            "StopSimulation",
            "Sinks",
            "Stops the simulation when its input is nonzero.",
            vec![],
        ),
        // Math
        d(
            "Gain",
            "Math",
            "Multiplies by a scalar, vector (elementwise) or matrix (K*u).",
            vec![p("gain", "Gain", "1")],
        ),
        d(
            "Sum",
            "Math",
            "Adds and subtracts inputs. Signs like \"+-\" or \"++-\".",
            vec![t("signs", "List of signs", "+-")],
        ),
        d(
            "Product",
            "Math",
            "Multiplies and divides inputs elementwise. Ops like \"**\" or \"*/\".",
            vec![t("ops", "Operations", "**")],
        ),
        d(
            "Bias",
            "Math",
            "Adds a constant.",
            vec![p("bias", "Bias", "0")],
        ),
        d("Abs", "Math", "Absolute value.", vec![]),
        d("Sign", "Math", "Sign of the input (-1, 0, 1).", vec![]),
        d(
            "MathFunction",
            "Math",
            "exp, log, log10, square, sqrt, reciprocal, pow10.",
            vec![c(
                "function",
                "Function",
                "exp",
                "choice:exp|log|log10|square|sqrt|reciprocal|pow10",
            )],
        ),
        d(
            "TrigFunction",
            "Math",
            "Trigonometric function of the input.",
            vec![c(
                "function",
                "Function",
                "sin",
                "choice:sin|cos|tan|asin|acos|atan|sinh|cosh|tanh",
            )],
        ),
        d(
            "MinMax",
            "Math",
            "Minimum or maximum over the inputs (or over one vector input).",
            vec![
                c("function", "Function", "min", "choice:min|max"),
                p("inputs", "Number of inputs", "2"),
            ],
        ),
        d("DotProduct", "Math", "sum(u1 .* u2).", vec![]),
        d(
            "Fcn",
            "Math",
            "Expression in u, e.g. u(1)^2 + sin(u(2)).",
            vec![t("expression", "Expression", "sin(u(1))")],
        ),
        d(
            "UserFunction",
            "Math",
            "Calls a .m function with the inputs as arguments.",
            vec![
                t("function", "Function name", "myfun"),
                p("inputs", "Number of inputs", "1"),
            ],
        ),
        // Continuous
        d(
            "Integrator",
            "Continuous",
            "Integrates its input. Optional limits, reset and state port.",
            vec![
                p("initial", "Initial condition", "0"),
                p("upper", "Upper limit", "inf"),
                p("lower", "Lower limit", "-inf"),
                c(
                    "reset",
                    "External reset",
                    "none",
                    "choice:none|rising|falling|either",
                ),
                c(
                    "ic_source",
                    "Initial condition source",
                    "internal",
                    "choice:internal|external",
                ),
                c("state_port", "Show state port", "off", "choice:off|on"),
            ],
        ),
        d(
            "Derivative",
            "Continuous",
            "Numerical time derivative of the input.",
            vec![],
        ),
        d(
            "TransferFcn",
            "Continuous",
            "num(s)/den(s), coefficients in descending powers of s.",
            vec![
                p("num", "Numerator", "[1]"),
                p("den", "Denominator", "[1 1]"),
            ],
        ),
        d(
            "StateSpace",
            "Continuous",
            "x' = Ax + Bu, y = Cx + Du.",
            vec![
                p("A", "A", "-1"),
                p("B", "B", "1"),
                p("C", "C", "1"),
                p("D", "D", "0"),
                p("x0", "Initial state", "0"),
            ],
        ),
        d(
            "ZeroPole",
            "Continuous",
            "k * prod(s - z) / prod(s - p).",
            vec![
                p("zeros", "Zeros", "[]"),
                p("poles", "Poles", "[-1]"),
                p("gain", "Gain", "1"),
            ],
        ),
        d(
            "TransportDelay",
            "Continuous",
            "Delays the input by a fixed time.",
            vec![
                p("delay", "Time delay", "1"),
                p("initial", "Initial output", "0"),
            ],
        ),
        d(
            "PID",
            "Continuous",
            "P + I/s + D*N/(1 + N/s) on the error input.",
            vec![
                p("P", "Proportional (P)", "1"),
                p("I", "Integral (I)", "1"),
                p("D", "Derivative (D)", "0"),
                p("N", "Filter coefficient (N)", "100"),
                p("initial", "Integrator initial state", "0"),
            ],
        ),
        // Discrete
        d(
            "UnitDelay",
            "Discrete",
            "Holds the input for one sample period.",
            vec![
                p("initial", "Initial condition", "0"),
                p("ts", "Sample time (period or [period offset])", "1"),
            ],
        ),
        d(
            "DiscreteTransferFcn",
            "Discrete",
            "num(z)/den(z), coefficients in descending powers of z.",
            vec![
                p("num", "Numerator", "[1]"),
                p("den", "Denominator", "[1 0.5]"),
                p("ts", "Sample time (period or [period offset])", "1"),
            ],
        ),
        d(
            "DiscreteStateSpace",
            "Discrete",
            "x[k+1] = Ax + Bu, y = Cx + Du.",
            vec![
                p("A", "A", "0.5"),
                p("B", "B", "1"),
                p("C", "C", "1"),
                p("D", "D", "0"),
                p("x0", "Initial state", "0"),
                p("ts", "Sample time (period or [period offset])", "1"),
            ],
        ),
        d(
            "DiscreteIntegrator",
            "Discrete",
            "Forward Euler: y[k+1] = y[k] + K*Ts*u[k].",
            vec![
                p("gain", "Gain K", "1"),
                p("initial", "Initial condition", "0"),
                p("ts", "Sample time (period or [period offset])", "1"),
            ],
        ),
        d(
            "ZeroOrderHold",
            "Discrete",
            "Samples and holds the input.",
            vec![p("ts", "Sample time (period or [period offset])", "1")],
        ),
        d(
            "DiscretePID",
            "Discrete",
            "Discrete PID with forward-Euler integral and backward-difference derivative.",
            vec![
                p("P", "Proportional (P)", "1"),
                p("I", "Integral (I)", "1"),
                p("D", "Derivative (D)", "0"),
                p("ts", "Sample time (period or [period offset])", "0.1"),
            ],
        ),
        // Nonlinear
        d(
            "Saturation",
            "Nonlinear",
            "Limits the input to [lower, upper].",
            vec![
                p("upper", "Upper limit", "0.5"),
                p("lower", "Lower limit", "-0.5"),
            ],
        ),
        d(
            "DeadZone",
            "Nonlinear",
            "Zero output between start and end.",
            vec![
                p("start", "Start of dead zone", "-0.5"),
                p("end", "End of dead zone", "0.5"),
            ],
        ),
        d(
            "Relay",
            "Nonlinear",
            "Switches between on and off values with hysteresis.",
            vec![
                p("on_point", "Switch on point", "0.1"),
                p("off_point", "Switch off point", "-0.1"),
                p("on_value", "Output when on", "1"),
                p("off_value", "Output when off", "0"),
            ],
        ),
        d(
            "RateLimiter",
            "Nonlinear",
            "Limits how fast the output can change.",
            vec![
                p("rising", "Rising slew rate", "1"),
                p("falling", "Falling slew rate", "-1"),
            ],
        ),
        d(
            "Quantizer",
            "Nonlinear",
            "Rounds to multiples of the interval.",
            vec![p("interval", "Quantization interval", "0.5")],
        ),
        d(
            "Lookup1D",
            "Nonlinear",
            "Linear interpolation in a table.",
            vec![
                p("breakpoints", "Breakpoints", "[-5:5]"),
                p("table", "Table data", "tanh([-5:5])"),
            ],
        ),
        // Routing and logic
        d(
            "Mux",
            "Routing",
            "Combines inputs into one vector.",
            vec![p("inputs", "Number of inputs", "2")],
        ),
        d(
            "Demux",
            "Routing",
            "Splits a vector into scalar outputs.",
            vec![p("outputs", "Number of outputs", "2")],
        ),
        d(
            "Switch",
            "Routing",
            "Passes input 1 when input 2 meets the criterion, else input 3.",
            vec![
                c(
                    "criteria",
                    "Criteria",
                    "u2 >= threshold",
                    "choice:u2 >= threshold|u2 > threshold|u2 ~= 0",
                ),
                p("threshold", "Threshold", "0"),
            ],
        ),
        d(
            "MultiportSwitch",
            "Routing",
            "Input 1 picks which data input passes (1-based, rounded and clamped).",
            vec![p("inputs", "Number of data inputs", "2")],
        ),
        d(
            "BusCreator",
            "Routing",
            "Bundles its inputs into a bus with named elements.",
            vec![t("names", "Element names (comma-separated)", "signal1, signal2")],
        ),
        d(
            "BusSelector",
            "Routing",
            "Picks named elements out of a bus, one output each.",
            vec![t("select", "Selected elements (comma-separated)", "signal1")],
        ),
        d(
            "Goto",
            "Routing",
            "Sends its input to every From block with the same tag, without a line.",
            vec![t("tag", "Tag", "A")],
        ),
        d(
            "From",
            "Routing",
            "Receives the signal of the Goto block with the same tag.",
            vec![t("tag", "Tag", "A")],
        ),
        d(
            "RelationalOperator",
            "Routing",
            "Compares two inputs.",
            vec![c("op", "Operator", "<=", "choice:==|~=|<|<=|>|>=")],
        ),
        d(
            "LogicalOperator",
            "Routing",
            "AND, OR, NOT, XOR of the inputs.",
            vec![
                c("op", "Operator", "AND", "choice:AND|OR|NOT|XOR|NAND|NOR"),
                p("inputs", "Number of inputs", "2"),
            ],
        ),
        // Structure
        d(
            "Subsystem",
            "Structure",
            "Groups blocks. Double-click to open.",
            vec![],
        ),
        d(
            "EnabledSubsystem",
            "Structure",
            "Subsystem that runs only while its enable input (last port) is positive; outputs and states hold otherwise.",
            vec![],
        ),
        d(
            "Inport",
            "Structure",
            "Subsystem input.",
            vec![p("port", "Port number", "1")],
        ),
    ]
}

/// Port counts for a block with the given (unevaluated) parameters, as the editor shows them.
/// Split a comma-separated list of bus element names.
pub fn bus_names(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

pub fn ports_for(
    kind: &str,
    params: &BTreeMap<String, String>,
    sub_ports: Option<(usize, usize)>,
) -> (usize, usize) {
    let n = |k: &str, d: usize| {
        params
            .get(k)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|x| x.max(0.0) as usize)
            .unwrap_or(d)
    };
    let txt = |k: &str, d: &str| params.get(k).cloned().unwrap_or_else(|| d.to_string());
    match kind {
        "Constant" | "Step" | "Ramp" | "SineWave" | "PulseGenerator" | "SignalGenerator"
        | "Clock" | "FromWorkspace" | "RandomNumber" => (0, 1),
        "Inport" => (0, 1),
        "Scope" => (n("inputs", 1).max(1), 0),
        "ToWorkspace" | "Display" | "Outport" | "Terminator" | "StopSimulation" => (1, 0),
        "Sum" => (
            txt("signs", "+-")
                .chars()
                .filter(|c| *c == '+' || *c == '-')
                .count()
                .max(1),
            1,
        ),
        "Product" => (
            txt("ops", "**")
                .chars()
                .filter(|c| *c == '*' || *c == '/')
                .count()
                .max(1),
            1,
        ),
        "MinMax" | "LogicalOperator" if txt("op", "") == "NOT" => (1, 1),
        "MinMax" | "LogicalOperator" | "Mux" => (n("inputs", 2).max(1), 1),
        "UserFunction" => (n("inputs", 1).max(1), 1),
        "DotProduct" | "RelationalOperator" => (2, 1),
        "Demux" => (1, n("outputs", 2).max(1)),
        "Switch" => (3, 1),
        "Integrator" => {
            let reset = txt("reset", "none") != "none";
            let ext = txt("ic_source", "internal") == "external";
            let state = txt("state_port", "off") == "on";
            (1 + reset as usize + ext as usize, 1 + state as usize)
        }
        "Subsystem" => sub_ports.unwrap_or((1, 1)),
        "EnabledSubsystem" => {
            let (i, o) = sub_ports.unwrap_or((1, 1));
            (i + 1, o)
        }
        "MultiportSwitch" => (n("inputs", 2).max(1) + 1, 1),
        "BusCreator" => (bus_names(&txt("names", "signal1, signal2")).len().max(1), 1),
        // The engine adds one hidden input per bus element (see `_elements`).
        "BusSelector" => (
            1 + n("_elements", 0),
            bus_names(&txt("select", "signal1")).len().max(1),
        ),
        "Goto" => (1, 0),
        "From" => (0, 1),
        _ => (1, 1),
    }
}

// ---------------------------------------------------------------- parameters

/// Evaluated block parameters.
pub struct Params {
    pub block: String,
    values: BTreeMap<String, Value>,
    texts: BTreeMap<String, String>,
}

impl Params {
    pub fn new(
        block: String,
        values: BTreeMap<String, Value>,
        texts: BTreeMap<String, String>,
    ) -> Self {
        Params {
            block,
            values,
            texts,
        }
    }
    fn err(&self, msg: String) -> String {
        format!("{}: {}", self.block, msg)
    }
    pub fn value(&self, k: &str) -> SimResult<&Value> {
        self.values
            .get(k)
            .ok_or_else(|| self.err(format!("missing parameter '{}'", k)))
    }
    pub fn vec(&self, k: &str) -> SimResult<Vec<f64>> {
        self.value(k)?
            .to_num()
            .map(|a| a.data)
            .map_err(|e| self.err(format!("parameter '{}': {}", k, e.message)))
    }
    pub fn mat(&self, k: &str) -> SimResult<Array<f64>> {
        self.value(k)?
            .to_num()
            .map_err(|e| self.err(format!("parameter '{}': {}", k, e.message)))
    }
    pub fn num(&self, k: &str) -> SimResult<f64> {
        let v = self.vec(k)?;
        v.first()
            .copied()
            .ok_or_else(|| self.err(format!("parameter '{}' is empty", k)))
    }
    pub fn text(&self, k: &str) -> String {
        self.texts.get(k).cloned().unwrap_or_default()
    }
    pub fn count(&self, k: &str, default: usize) -> usize {
        self.num(k).map(|x| x.max(0.0) as usize).unwrap_or(default)
    }
}

// ---------------------------------------------------------------- helpers

fn first(u: &[Vec<f64>]) -> &[f64] {
    u.first().map(|v| v.as_slice()).unwrap_or(&[])
}

/// Elementwise combine with scalar expansion.
fn zip2(a: &[f64], b: &[f64], f: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| {
            f(
                a[if a.len() == 1 {
                    0
                } else {
                    i.min(a.len().saturating_sub(1))
                }],
                b[if b.len() == 1 {
                    0
                } else {
                    i.min(b.len().saturating_sub(1))
                }],
            )
        })
        .collect()
}

fn map(a: &[f64], f: impl Fn(f64) -> f64) -> Vec<f64> {
    a.iter().map(|x| f(*x)).collect()
}

fn discrete(period: f64) -> Sample {
    Sample::Discrete {
        period,
        offset: 0.0,
    }
}

/// Controllable canonical realization of num/den (descending powers). Returns (A, B, C, D) with A row-major n×n.
pub fn tf_to_ss(num: &[f64], den: &[f64]) -> SimResult<(Vec<f64>, Vec<f64>, Vec<f64>, f64, usize)> {
    let k = den
        .iter()
        .position(|x| *x != 0.0)
        .ok_or("denominator must not be zero")?;
    let den = &den[k..];
    let n = den.len() - 1;
    let k = num
        .iter()
        .position(|x| *x != 0.0)
        .unwrap_or(num.len().saturating_sub(1));
    let num = &num[k.min(num.len().saturating_sub(1))..];
    if num.len() > den.len() {
        return Err(
            "transfer function must be proper (numerator degree <= denominator degree)".into(),
        );
    }
    let a0 = den[0];
    let a: Vec<f64> = den.iter().map(|x| x / a0).collect();
    let mut b = vec![0.0; n + 1 - num.len()];
    b.extend(num.iter().map(|x| x / a0));
    let mut am = vec![0.0; n * n];
    for j in 0..n {
        am[j] = -a[j + 1];
    }
    for i in 1..n {
        am[i * n + i - 1] = 1.0;
    }
    let mut bm = vec![0.0; n];
    if n > 0 {
        bm[0] = 1.0;
    }
    let cm: Vec<f64> = (0..n).map(|j| b[j + 1] - a[j + 1] * b[0]).collect();
    Ok((am, bm, cm, b[0], n))
}

fn poly_from_roots(r: &[f64]) -> Vec<f64> {
    let mut p = vec![1.0];
    for x in r {
        let mut q = vec![0.0; p.len() + 1];
        for (i, c) in p.iter().enumerate() {
            q[i] += c;
            q[i + 1] -= c * x;
        }
        p = q;
    }
    p
}

// ---------------------------------------------------------------- factory

pub fn create(kind: &str, p: &Params, ports: (usize, usize)) -> SimResult<Box<dyn Block>> {
    Ok(match kind {
        "Constant" => Box::new(Constant { v: p.vec("value")? }),
        "Step" => Box::new(Step {
            time: p.num("time")?,
            before: p.vec("initial")?,
            after: p.vec("final")?,
        }),
        "Ramp" => Box::new(Ramp {
            slope: p.num("slope")?,
            start: p.num("start")?,
            initial: p.num("initial")?,
        }),
        "SineWave" => Box::new(Sine {
            a: p.num("amplitude")?,
            bias: p.num("bias")?,
            w: p.num("frequency")?,
            ph: p.num("phase")?,
        }),
        "PulseGenerator" => Box::new(Pulse {
            amp: p.num("amplitude")?,
            period: p.num("period")?.max(1e-12),
            width: p.num("width")? / 100.0,
            delay: p.num("delay")?,
        }),
        "SignalGenerator" => Box::new(SignalGen {
            wave: p.text("wave"),
            amp: p.num("amplitude")?,
            f: p.num("frequency")?,
        }),
        "Clock" => Box::new(Clock),
        "FromWorkspace" => {
            let m = p.mat("data")?;
            if m.cols < 2 || m.rows < 1 {
                return Err(format!(
                    "{}: data must be a matrix [t, y1, ...] with at least two columns",
                    p.block
                ));
            }
            Box::new(FromWs { m })
        }
        "RandomNumber" => {
            let var = p.num("variance")?.max(0.0);
            Box::new(Random {
                dist: Normal::new(p.num("mean")?, var.sqrt()).map_err(|e| e.to_string())?,
                rng: StdRng::seed_from_u64(p.num("seed")? as u64),
                ts: p.num("ts")?,
                value: 0.0,
            })
        }
        "Scope" => Box::new(Sink {
            n: ports.0,
            kind: LogKind::Scope {
                separate: p.text("layout") == "one per input",
            },
        }),
        "ToWorkspace" => {
            let v = p.text("variable");
            if !crate::lexer::is_identifier(&v) {
                return Err(format!("{}: '{}' is not a valid variable name", p.block, v));
            }
            Box::new(Sink {
                n: 1,
                kind: LogKind::ToWorkspace(v),
            })
        }
        "Display" => Box::new(Sink {
            n: 1,
            kind: LogKind::Display,
        }),
        "Outport" => Box::new(Sink {
            n: 1,
            kind: LogKind::Outport(p.count("port", 1)),
        }),
        "Terminator" => Box::new(Terminator),
        "StopSimulation" => Box::new(Stop),
        "Gain" => Box::new(Gain { k: p.mat("gain")? }),
        "Sum" => {
            let signs: Vec<f64> = p
                .text("signs")
                .chars()
                .filter_map(|c| match c {
                    '+' => Some(1.0),
                    '-' => Some(-1.0),
                    _ => None,
                })
                .collect();
            Box::new(Sum {
                signs: if signs.is_empty() { vec![1.0] } else { signs },
            })
        }
        "Product" => {
            let ops: Vec<bool> = p
                .text("ops")
                .chars()
                .filter_map(|c| match c {
                    '*' => Some(true),
                    '/' => Some(false),
                    _ => None,
                })
                .collect();
            Box::new(Product {
                mul: if ops.is_empty() { vec![true] } else { ops },
            })
        }
        "Bias" => Box::new(Unary {
            f: Unaries::Bias(p.num("bias")?),
        }),
        "Abs" => Box::new(Unary { f: Unaries::Abs }),
        "Sign" => Box::new(Unary { f: Unaries::Sign }),
        "MathFunction" => Box::new(Unary {
            f: Unaries::Named(p.text("function")),
        }),
        "TrigFunction" => Box::new(Unary {
            f: Unaries::Named(p.text("function")),
        }),
        "MinMax" => Box::new(MinMax {
            max: p.text("function") == "max",
            n: ports.0,
        }),
        "DotProduct" => Box::new(Dot),
        "Fcn" => {
            let src = p.text("expression");
            let expr = crate::parser::parse_expression(&src)
                .map_err(|e| format!("{}: {}", p.block, e.message))?;
            Box::new(Fcn {
                expr,
                name: p.block.clone(),
            })
        }
        "UserFunction" => Box::new(UserFn {
            name: p.text("function"),
            n: ports.0,
            block: p.block.clone(),
        }),
        "Integrator" => {
            let reset = p.text("reset");
            Box::new(Integrator {
                x0: p.vec("initial")?,
                upper: p.num("upper")?,
                lower: p.num("lower")?,
                reset: match reset.as_str() {
                    "rising" => 1,
                    "falling" => 2,
                    "either" => 3,
                    _ => 0,
                },
                external_ic: p.text("ic_source") == "external",
                state_port: p.text("state_port") == "on",
            })
        }
        "Derivative" => Box::new(Deriv {
            last_t: f64::NAN,
            last_u: vec![],
            out: vec![0.0],
        }),
        "TransferFcn" => {
            let (a, b, cc, d, n) = tf_to_ss(&p.vec("num")?, &p.vec("den")?)
                .map_err(|e| format!("{}: {}", p.block, e))?;
            Box::new(Ss::new(n, a, b, cc, vec![d], 1, 1, vec![0.0; n]))
        }
        "ZeroPole" => {
            let k = p.num("gain")?;
            let num: Vec<f64> = poly_from_roots(&p.vec("zeros")?)
                .iter()
                .map(|x| x * k)
                .collect();
            let den = poly_from_roots(&p.vec("poles")?);
            let (a, b, cc, d, n) =
                tf_to_ss(&num, &den).map_err(|e| format!("{}: {}", p.block, e))?;
            Box::new(Ss::new(n, a, b, cc, vec![d], 1, 1, vec![0.0; n]))
        }
        "StateSpace" => {
            let (a, b, cc, d, x0) = (
                p.mat("A")?,
                p.mat("B")?,
                p.mat("C")?,
                p.mat("D")?,
                p.vec("x0")?,
            );
            let n = a.rows;
            if a.cols != n || b.rows != n || cc.cols != n {
                return Err(format!("{}: A, B, C sizes do not agree", p.block));
            }
            let ni = b.cols;
            let no = cc.rows;
            let row = |m: &Array<f64>| -> Vec<f64> {
                (0..m.rows)
                    .flat_map(|r| (0..m.cols).map(move |c2| (r, c2)))
                    .map(|(r, c2)| *m.get(r, c2))
                    .collect()
            };
            let d = if d.numel() == 1 && ni * no > 1 && d.data[0] == 0.0 {
                vec![0.0; ni * no]
            } else {
                row(&d)
            };
            let x0 = if x0.len() == 1 { vec![x0[0]; n] } else { x0 };
            Box::new(Ss::new(n, row(&a), row(&b), row(&cc), d, ni, no, x0))
        }
        "TransportDelay" => Box::new(Delay {
            delay: p.num("delay")?.max(0.0),
            init: p.num("initial")?,
            hist: VecDeque::new(),
        }),
        "PID" => Box::new(Pid {
            p: p.num("P")?,
            i: p.num("I")?,
            d: p.num("D")?,
            n: p.num("N")?.max(1e-9),
            x0: p.num("initial")?,
        }),
        "UnitDelay" => Box::new(UnitDelay {
            x: p.vec("initial")?,
            ts: p.num("ts")?,
        }),
        "DiscreteTransferFcn" => {
            let (a, b, cc, d, n) = tf_to_ss(&p.vec("num")?, &p.vec("den")?)
                .map_err(|e| format!("{}: {}", p.block, e))?;
            Box::new(DSs {
                n,
                a,
                b,
                c: cc,
                d: vec![d],
                ni: 1,
                no: 1,
                x: vec![0.0; n],
                ts: p.num("ts")?,
            })
        }
        "DiscreteStateSpace" => {
            let (a, b, cc, d, x0) = (
                p.mat("A")?,
                p.mat("B")?,
                p.mat("C")?,
                p.mat("D")?,
                p.vec("x0")?,
            );
            let n = a.rows;
            let row = |m: &Array<f64>| -> Vec<f64> {
                (0..m.rows)
                    .flat_map(|r| (0..m.cols).map(move |c2| (r, c2)))
                    .map(|(r, c2)| *m.get(r, c2))
                    .collect()
            };
            let ni = b.cols;
            let no = cc.rows;
            let d = if d.numel() == 1 && ni * no > 1 {
                vec![d.data[0]; ni * no]
            } else {
                row(&d)
            };
            Box::new(DSs {
                n,
                a: row(&a),
                b: row(&b),
                c: row(&cc),
                d,
                ni,
                no,
                x: if x0.len() == 1 { vec![x0[0]; n] } else { x0 },
                ts: p.num("ts")?,
            })
        }
        "DiscreteIntegrator" => Box::new(DInt {
            k: p.num("gain")?,
            x: p.vec("initial")?,
            ts: p.num("ts")?,
        }),
        "ZeroOrderHold" => Box::new(Zoh { ts: p.num("ts")? }),
        "DiscretePID" => Box::new(DPid {
            p: p.num("P")?,
            i: p.num("I")?,
            d: p.num("D")?,
            ts: p.num("ts")?,
            integ: 0.0,
            prev: f64::NAN,
        }),
        "Saturation" => Box::new(Unary {
            f: Unaries::Sat(p.num("lower")?, p.num("upper")?),
        }),
        "DeadZone" => Box::new(Unary {
            f: Unaries::Dead(p.num("start")?, p.num("end")?),
        }),
        "Quantizer" => Box::new(Unary {
            f: Unaries::Quant(p.num("interval")?),
        }),
        "Relay" => Box::new(Relay {
            on: p.num("on_point")?,
            off: p.num("off_point")?,
            von: p.num("on_value")?,
            voff: p.num("off_value")?,
            state: false,
        }),
        "RateLimiter" => Box::new(Rate {
            up: p.num("rising")?,
            down: p.num("falling")?,
            last_t: f64::NAN,
            last_y: vec![],
        }),
        "Lookup1D" => {
            let bp = p.vec("breakpoints")?;
            let tb = p.vec("table")?;
            if bp.len() != tb.len() || bp.len() < 2 {
                return Err(format!(
                    "{}: breakpoints and table must have the same length (at least 2)",
                    p.block
                ));
            }
            Box::new(Lookup { bp, tb })
        }
        "Mux" | "BusCreator" => Box::new(Mux { n: ports.0 }),
        "BusSelector" => Box::new(BusSelect {
            pick: p
                .text("_pick")
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect(),
            n_in: ports.0,
        }),
        "Demux" => Box::new(Demux { n: ports.1 }),
        "Switch" => Box::new(Switch {
            mode: p.text("criteria"),
            th: p.num("threshold")?,
        }),
        "RelationalOperator" => Box::new(Rel { op: p.text("op") }),
        "LogicalOperator" => Box::new(Logic {
            op: p.text("op"),
            n: ports.0,
        }),
        "MultiportSwitch" => Box::new(MultiSwitch {
            n: ports.0.saturating_sub(1).max(1),
        }),
        "Inport" | "Pass" | "Goto" | "From" => Box::new(Pass),
        other => return Err(format!("Unknown block type '{}'.", other)),
    })
}

// ---------------------------------------------------------------- sources

struct Constant {
    v: Vec<f64>,
}
impl Block for Constant {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn sample(&self) -> Sample {
        Sample::Constant
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = self.v.clone();
        Ok(())
    }
}

struct Step {
    time: f64,
    before: Vec<f64>,
    after: Vec<f64>,
}
impl Block for Step {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = if t >= self.time {
            self.after.clone()
        } else {
            self.before.clone()
        };
        Ok(())
    }
    fn zero_crossings(&self, t: f64, _: &[f64], _: &[Vec<f64>]) -> Vec<f64> {
        vec![t - self.time]
    }
}

struct Ramp {
    slope: f64,
    start: f64,
    initial: f64,
}
impl Block for Ramp {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = vec![self.initial + self.slope * (t - self.start).max(0.0)];
        Ok(())
    }
    fn zero_crossings(&self, t: f64, _: &[f64], _: &[Vec<f64>]) -> Vec<f64> {
        vec![t - self.start]
    }
}

struct Sine {
    a: f64,
    bias: f64,
    w: f64,
    ph: f64,
}
impl Block for Sine {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = vec![self.a * (self.w * t + self.ph).sin() + self.bias];
        Ok(())
    }
}

struct Pulse {
    amp: f64,
    period: f64,
    width: f64,
    delay: f64,
}
impl Block for Pulse {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let on = t >= self.delay
            && ((t - self.delay).rem_euclid(self.period) < self.width * self.period - 1e-12);
        y[0] = vec![if on { self.amp } else { 0.0 }];
        Ok(())
    }
    fn zero_crossings(&self, t: f64, _: &[f64], _: &[Vec<f64>]) -> Vec<f64> {
        // Distance to the next edge, signed so it flips exactly at each edge.
        if t < self.delay {
            return vec![t - self.delay, 1.0];
        }
        let ph = (t - self.delay).rem_euclid(self.period);
        vec![ph - self.width * self.period, ph - self.period * 0.999_999]
    }
}

struct SignalGen {
    wave: String,
    amp: f64,
    f: f64,
}
impl Block for SignalGen {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let ph = (t * self.f).rem_euclid(1.0);
        let v = match self.wave.as_str() {
            "square" => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            "sawtooth" => 2.0 * ph - 1.0,
            _ => (2.0 * std::f64::consts::PI * self.f * t).sin(),
        };
        y[0] = vec![self.amp * v];
        Ok(())
    }
}

struct Clock;
impl Block for Clock {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = vec![t];
        Ok(())
    }
}

struct FromWs {
    m: Array<f64>,
}
impl Block for FromWs {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let m = &self.m;
        let n = m.rows;
        let tt = |r: usize| *m.get(r, 0);
        let mut k = 0;
        while k + 1 < n && tt(k + 1) <= t {
            k += 1;
        }
        let row = |r: usize| -> Vec<f64> { (1..m.cols).map(|c| *m.get(r, c)).collect() };
        y[0] = if t <= tt(0) || n == 1 {
            row(0)
        } else if k + 1 >= n {
            row(n - 1)
        } else {
            let f = (t - tt(k)) / (tt(k + 1) - tt(k));
            zip2(&row(k), &row(k + 1), |a, b| a + f * (b - a))
        };
        Ok(())
    }
}

struct Random {
    dist: Normal<f64>,
    rng: StdRng,
    ts: f64,
    value: f64,
}
impl Block for Random {
    fn ports(&self) -> (usize, usize) {
        (0, 1)
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        self.value = self.dist.sample(&mut self.rng);
        y[0] = vec![self.value];
        Ok(())
    }
}

// ---------------------------------------------------------------- sinks

struct Sink {
    n: usize,
    kind: LogKind,
}
impl Block for Sink {
    fn ports(&self) -> (usize, usize) {
        (self.n, 0)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        _: &mut [Vec<f64>],
    ) -> SimResult<()> {
        Ok(())
    }
    fn log(&self) -> Option<LogKind> {
        Some(self.kind.clone())
    }
}

struct Terminator;
impl Block for Terminator {
    fn ports(&self) -> (usize, usize) {
        (1, 0)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        _: &mut [Vec<f64>],
    ) -> SimResult<()> {
        Ok(())
    }
}

struct Stop;
impl Block for Stop {
    fn ports(&self) -> (usize, usize) {
        (1, 0)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        _: &mut [Vec<f64>],
    ) -> SimResult<()> {
        Ok(())
    }
    fn stops(&self, u: &[Vec<f64>]) -> bool {
        first(u).iter().any(|v| *v != 0.0)
    }
}

// ---------------------------------------------------------------- math

struct Gain {
    k: Array<f64>,
}
impl Block for Gain {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u);
        let k = &self.k;
        // A true matrix gain computes K*u; scalars and vectors multiply elementwise.
        y[0] = if k.rows > 1 && k.cols > 1 {
            if k.cols != u.len() {
                return Err(format!(
                    "matrix gain is {}x{} but the input has {} elements",
                    k.rows,
                    k.cols,
                    u.len()
                ));
            }
            (0..k.rows)
                .map(|r| (0..k.cols).map(|c2| k.get(r, c2) * u[c2]).sum())
                .collect()
        } else {
            zip2(&k.data, u, |a, b| a * b)
        };
        Ok(())
    }
}

struct Sum {
    signs: Vec<f64>,
}
impl Block for Sum {
    fn ports(&self) -> (usize, usize) {
        (self.signs.len(), 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        if self.signs.len() == 1 {
            // One input: sum over its elements.
            y[0] = vec![self.signs[0] * first(u).iter().sum::<f64>()];
            return Ok(());
        }
        let mut acc = vec![0.0];
        for (s, v) in self.signs.iter().zip(u) {
            acc = zip2(&acc, v, |a, b| a + s * b);
        }
        y[0] = acc;
        Ok(())
    }
}

struct Product {
    mul: Vec<bool>,
}
impl Block for Product {
    fn ports(&self) -> (usize, usize) {
        (self.mul.len(), 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        if self.mul.len() == 1 {
            y[0] = vec![first(u).iter().product::<f64>()];
            return Ok(());
        }
        let mut acc = vec![1.0];
        for (m, v) in self.mul.iter().zip(u) {
            acc = if *m {
                zip2(&acc, v, |a, b| a * b)
            } else {
                zip2(&acc, v, |a, b| a / b)
            };
        }
        y[0] = acc;
        Ok(())
    }
}

enum Unaries {
    Bias(f64),
    Abs,
    Sign,
    Named(String),
    Sat(f64, f64),
    Dead(f64, f64),
    Quant(f64),
}

struct Unary {
    f: Unaries,
}
impl Block for Unary {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u);
        y[0] = match &self.f {
            Unaries::Bias(b) => map(u, |x| x + b),
            Unaries::Abs => map(u, f64::abs),
            Unaries::Sign => map(u, |x| {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }),
            Unaries::Sat(lo, hi) => map(u, |x| x.clamp(*lo, *hi)),
            Unaries::Dead(s, e) => map(u, |x| {
                if x < *s {
                    x - s
                } else if x > *e {
                    x - e
                } else {
                    0.0
                }
            }),
            Unaries::Quant(q) => map(u, |x| if *q > 0.0 { (x / q).round() * q } else { x }),
            Unaries::Named(n) => {
                let f: fn(f64) -> f64 = match n.as_str() {
                    "exp" => f64::exp,
                    "log" => f64::ln,
                    "log10" => f64::log10,
                    "square" => |x| x * x,
                    "sqrt" => f64::sqrt,
                    "reciprocal" => |x| 1.0 / x,
                    "pow10" => |x| 10f64.powf(x),
                    "sin" => f64::sin,
                    "cos" => f64::cos,
                    "tan" => f64::tan,
                    "asin" => f64::asin,
                    "acos" => f64::acos,
                    "atan" => f64::atan,
                    "sinh" => f64::sinh,
                    "cosh" => f64::cosh,
                    "tanh" => f64::tanh,
                    other => return Err(format!("unknown function '{}'", other)),
                };
                map(u, f)
            }
        };
        Ok(())
    }
    fn zero_crossings(&self, _: f64, _: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        let u = first(u);
        match &self.f {
            Unaries::Abs | Unaries::Sign => u.to_vec(),
            Unaries::Sat(lo, hi) => u.iter().flat_map(|x| [x - lo, x - hi]).collect(),
            Unaries::Dead(s, e) => u.iter().flat_map(|x| [x - s, x - e]).collect(),
            _ => Vec::new(),
        }
    }
}

struct MinMax {
    max: bool,
    n: usize,
}
impl Block for MinMax {
    fn ports(&self) -> (usize, usize) {
        (self.n, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let pick = |a: f64, b: f64| if self.max { a.max(b) } else { a.min(b) };
        y[0] = if self.n == 1 {
            vec![first(u).iter().copied().fold(
                if self.max {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                pick,
            )]
        } else {
            let mut acc = u[0].clone();
            for v in &u[1..] {
                acc = zip2(&acc, v, pick);
            }
            acc
        };
        Ok(())
    }
}

struct Dot;
impl Block for Dot {
    fn ports(&self) -> (usize, usize) {
        (2, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = vec![zip2(&u[0], &u[1], |a, b| a * b).iter().sum()];
        Ok(())
    }
}

struct Fcn {
    expr: crate::ast::Expr,
    name: String,
}
impl Block for Fcn {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        ctx: &mut Ctx,
        t: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let it = &mut *ctx.interp;
        let saved_u = it.get_var("u").cloned();
        let saved_t = it.get_var("t").cloned();
        it.set_var("u", Value::col(first(u).to_vec()));
        it.set_var("t", Value::num(t));
        let r = it.eval(&self.expr);
        match saved_u {
            Some(v) => it.set_var("u", v),
            None => it.clear_var("u"),
        }
        match saved_t {
            Some(v) => it.set_var("t", v),
            None => it.clear_var("t"),
        }
        let v = r.map_err(|e| format!("{}: {}", self.name, e.message))?;
        y[0] = v
            .to_num()
            .map_err(|e| format!("{}: {}", self.name, e.message))?
            .data;
        Ok(())
    }
}

struct UserFn {
    name: String,
    n: usize,
    block: String,
}
impl Block for UserFn {
    fn ports(&self) -> (usize, usize) {
        (self.n, 1)
    }
    fn output(
        &mut self,
        ctx: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let args: Vec<Value> = u
            .iter()
            .map(|v| {
                if v.len() == 1 {
                    Value::num(v[0])
                } else {
                    Value::col(v.clone())
                }
            })
            .collect();
        let r = ctx
            .interp
            .call_named(&self.name, args, 1)
            .map_err(|e| format!("{}: {}", self.block, e.report()))?;
        let v = r
            .into_iter()
            .next()
            .ok_or_else(|| format!("{}: function '{}' returned no value", self.block, self.name))?;
        y[0] = v
            .to_num()
            .map_err(|e| format!("{}: {}", self.block, e.message))?
            .data;
        Ok(())
    }
}

// ---------------------------------------------------------------- continuous

struct Integrator {
    x0: Vec<f64>,
    upper: f64,
    lower: f64,
    /// 0 none, 1 rising, 2 falling, 3 either.
    reset: u8,
    /// Reset loads the value on the last input port instead of the initial-condition parameter.
    external_ic: bool,
    state_port: bool,
}
impl Block for Integrator {
    fn ports(&self) -> (usize, usize) {
        (
            1 + (self.reset > 0) as usize + self.external_ic as usize,
            1 + self.state_port as usize,
        )
    }
    fn feedthrough(&self, _: usize) -> bool {
        false
    }
    fn nstates(&self) -> usize {
        self.x0.len().max(1)
    }
    fn init(&mut self, x: &mut [f64]) {
        for (i, v) in x.iter_mut().enumerate() {
            *v = *self.x0.get(i).or(self.x0.first()).unwrap_or(&0.0);
        }
        self.project(x);
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        x: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = x.to_vec();
        if self.state_port {
            y[1] = x.to_vec();
        }
        Ok(())
    }
    fn derivative(&self, _: f64, x: &[f64], u: &[Vec<f64>], dx: &mut [f64]) {
        let u = first(u);
        for (i, d) in dx.iter_mut().enumerate() {
            let v = *u.get(i).or(u.first()).unwrap_or(&0.0);
            // Hold at a limit while the input pushes further out.
            *d = if (x[i] >= self.upper && v > 0.0) || (x[i] <= self.lower && v < 0.0) {
                0.0
            } else {
                v
            };
        }
    }
    fn project(&self, x: &mut [f64]) {
        for v in x.iter_mut() {
            *v = v.clamp(self.lower, self.upper);
        }
    }
    fn zero_crossings(&self, _: f64, x: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        let mut z = Vec::new();
        if self.reset > 0 {
            z.push(u.get(1).and_then(|v| v.first()).copied().unwrap_or(0.0));
        }
        if self.upper.is_finite() {
            z.extend(x.iter().map(|v| v - self.upper));
        }
        if self.lower.is_finite() {
            z.extend(x.iter().map(|v| v - self.lower));
        }
        z
    }
    fn event(&mut self, _: f64, x: &mut [f64], u: &[Vec<f64>], which: usize, rising: bool) {
        let wanted = match self.reset {
            1 => rising,
            2 => !rising,
            3 => true,
            _ => false,
        };
        if wanted && which == 0 {
            let ic: Vec<f64> = if self.external_ic {
                u.last().cloned().unwrap_or_default()
            } else {
                self.x0.clone()
            };
            for (i, v) in x.iter_mut().enumerate() {
                *v = *ic.get(i).or(ic.first()).unwrap_or(&0.0);
            }
        }
        self.project(x);
    }
}

struct Deriv {
    last_t: f64,
    last_u: Vec<f64>,
    out: Vec<f64>,
}
impl Block for Deriv {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        ctx: &mut Ctx,
        t: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u).to_vec();
        if ctx.major {
            if self.last_t.is_finite() && t > self.last_t {
                self.out = zip2(&u, &self.last_u, |a, b| a - b)
                    .iter()
                    .map(|d| d / (t - self.last_t))
                    .collect();
            }
            self.last_t = t;
            self.last_u = u;
        }
        y[0] = self.out.clone();
        Ok(())
    }
}

/// Continuous state-space block (also used for transfer functions).
struct Ss {
    n: usize,
    a: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
    ni: usize,
    no: usize,
    x0: Vec<f64>,
}
impl Ss {
    #[allow(clippy::too_many_arguments)]
    fn new(
        n: usize,
        a: Vec<f64>,
        b: Vec<f64>,
        c: Vec<f64>,
        d: Vec<f64>,
        ni: usize,
        no: usize,
        x0: Vec<f64>,
    ) -> Self {
        Ss {
            n,
            a,
            b,
            c,
            d,
            ni,
            no,
            x0,
        }
    }
}
impl Block for Ss {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn feedthrough(&self, _: usize) -> bool {
        self.d.iter().any(|v| *v != 0.0)
    }
    fn nstates(&self) -> usize {
        self.n
    }
    fn init(&mut self, x: &mut [f64]) {
        for (i, v) in x.iter_mut().enumerate() {
            *v = self.x0.get(i).copied().unwrap_or(0.0);
        }
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        x: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u);
        let ui = |j: usize| *u.get(j).or(u.first()).unwrap_or(&0.0);
        y[0] = (0..self.no)
            .map(|r| {
                (0..self.n)
                    .map(|j| self.c[r * self.n + j] * x[j])
                    .sum::<f64>()
                    + (0..self.ni)
                        .map(|j| self.d[r * self.ni + j] * ui(j))
                        .sum::<f64>()
            })
            .collect();
        Ok(())
    }
    fn derivative(&self, _: f64, x: &[f64], u: &[Vec<f64>], dx: &mut [f64]) {
        let u = first(u);
        let ui = |j: usize| *u.get(j).or(u.first()).unwrap_or(&0.0);
        for i in 0..self.n {
            dx[i] = (0..self.n)
                .map(|j| self.a[i * self.n + j] * x[j])
                .sum::<f64>()
                + (0..self.ni)
                    .map(|j| self.b[i * self.ni + j] * ui(j))
                    .sum::<f64>();
        }
    }
}

struct Delay {
    delay: f64,
    init: f64,
    hist: VecDeque<(f64, Vec<f64>)>,
}
impl Block for Delay {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn feedthrough(&self, _: usize) -> bool {
        self.delay == 0.0
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        t: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let target = t - self.delay;
        if self.delay == 0.0 {
            y[0] = first(u).to_vec();
            return Ok(());
        }
        y[0] = if self.hist.is_empty() || target < self.hist[0].0 {
            vec![self.init]
        } else {
            let k = self
                .hist
                .iter()
                .rposition(|(tt, _)| *tt <= target)
                .unwrap_or(0);
            match self.hist.get(k + 1) {
                Some((t1, v1)) => {
                    let (t0, v0) = &self.hist[k];
                    let f = if t1 > t0 {
                        (target - t0) / (t1 - t0)
                    } else {
                        0.0
                    };
                    zip2(v0, v1, |a, b| a + f * (b - a))
                }
                None => self.hist[k].1.clone(),
            }
        };
        Ok(())
    }
    /// Records the input at every major step; history older than the delay is dropped.
    fn update(&mut self, t: f64, _: &mut [f64], u: &[Vec<f64>]) {
        self.hist.push_back((t, first(u).to_vec()));
        while self.hist.len() > 2 && self.hist[1].0 < t - self.delay - 1e-9 {
            self.hist.pop_front();
        }
    }
}

/// Parallel PID on the error input: integral state and filtered-derivative state.
struct Pid {
    p: f64,
    i: f64,
    d: f64,
    n: f64,
    x0: f64,
}
impl Block for Pid {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn nstates(&self) -> usize {
        2
    }
    fn init(&mut self, x: &mut [f64]) {
        x[0] = self.x0;
        x[1] = 0.0;
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        x: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let e = first(u).first().copied().unwrap_or(0.0);
        // Derivative term D*N*(e - xf), where xf' = N*(e - xf).
        y[0] = vec![self.p * e + self.i * x[0] + self.d * self.n * (e - x[1])];
        Ok(())
    }
    fn derivative(&self, _: f64, x: &[f64], u: &[Vec<f64>], dx: &mut [f64]) {
        let e = first(u).first().copied().unwrap_or(0.0);
        dx[0] = e;
        dx[1] = self.n * (e - x[1]);
    }
}

// ---------------------------------------------------------------- discrete

struct UnitDelay {
    x: Vec<f64>,
    ts: f64,
}
impl Block for UnitDelay {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn feedthrough(&self, _: usize) -> bool {
        false
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = self.x.clone();
        Ok(())
    }
    fn update(&mut self, _: f64, _: &mut [f64], u: &[Vec<f64>]) {
        self.x = first(u).to_vec();
    }
}

struct DSs {
    n: usize,
    a: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
    ni: usize,
    no: usize,
    x: Vec<f64>,
    ts: f64,
}
impl Block for DSs {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn feedthrough(&self, _: usize) -> bool {
        self.d.iter().any(|v| *v != 0.0)
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u);
        let ui = |j: usize| *u.get(j).or(u.first()).unwrap_or(&0.0);
        y[0] = (0..self.no)
            .map(|r| {
                (0..self.n)
                    .map(|j| self.c[r * self.n + j] * self.x[j])
                    .sum::<f64>()
                    + (0..self.ni)
                        .map(|j| self.d[r * self.ni + j] * ui(j))
                        .sum::<f64>()
            })
            .collect();
        Ok(())
    }
    fn update(&mut self, _: f64, _: &mut [f64], u: &[Vec<f64>]) {
        let u = first(u);
        let ui = |j: usize| *u.get(j).or(u.first()).unwrap_or(&0.0);
        self.x = (0..self.n)
            .map(|i| {
                (0..self.n)
                    .map(|j| self.a[i * self.n + j] * self.x[j])
                    .sum::<f64>()
                    + (0..self.ni)
                        .map(|j| self.b[i * self.ni + j] * ui(j))
                        .sum::<f64>()
            })
            .collect();
    }
}

struct DInt {
    k: f64,
    x: Vec<f64>,
    ts: f64,
}
impl Block for DInt {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn feedthrough(&self, _: usize) -> bool {
        false
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        _: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = self.x.clone();
        Ok(())
    }
    fn update(&mut self, _: f64, _: &mut [f64], u: &[Vec<f64>]) {
        let (k, ts) = (self.k, self.ts);
        self.x = zip2(&self.x, first(u), |x, v| x + k * ts * v);
    }
}

struct Zoh {
    ts: f64,
}
impl Block for Zoh {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = first(u).to_vec();
        Ok(())
    }
}

struct DPid {
    p: f64,
    i: f64,
    d: f64,
    ts: f64,
    integ: f64,
    prev: f64,
}
impl Block for DPid {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn sample(&self) -> Sample {
        discrete(self.ts)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let e = first(u).first().copied().unwrap_or(0.0);
        let de = if self.prev.is_nan() {
            0.0
        } else {
            (e - self.prev) / self.ts
        };
        y[0] = vec![self.p * e + self.i * self.integ + self.d * de];
        Ok(())
    }
    fn update(&mut self, _: f64, _: &mut [f64], u: &[Vec<f64>]) {
        let e = first(u).first().copied().unwrap_or(0.0);
        self.integ += self.ts * e;
        self.prev = e;
    }
}

// ---------------------------------------------------------------- nonlinear

struct Relay {
    on: f64,
    off: f64,
    von: f64,
    voff: f64,
    state: bool,
}
impl Block for Relay {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        ctx: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let v = first(u).first().copied().unwrap_or(0.0);
        if ctx.major {
            if v >= self.on {
                self.state = true;
            } else if v <= self.off {
                self.state = false;
            }
        }
        y[0] = vec![if self.state { self.von } else { self.voff }];
        Ok(())
    }
    fn zero_crossings(&self, _: f64, _: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        let v = first(u).first().copied().unwrap_or(0.0);
        vec![v - self.on, v - self.off]
    }
}

struct Rate {
    up: f64,
    down: f64,
    last_t: f64,
    last_y: Vec<f64>,
}
impl Block for Rate {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        ctx: &mut Ctx,
        t: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u).to_vec();
        let out = if self.last_t.is_nan() || t <= self.last_t {
            if self.last_t.is_nan() {
                u.clone()
            } else {
                self.last_y.clone()
            }
        } else {
            let dt = t - self.last_t;
            let (up, down) = (self.up, self.down);
            zip2(&u, &self.last_y, |v, last| {
                v.clamp(last + down * dt, last + up * dt)
            })
        };
        if ctx.major {
            self.last_t = t;
            self.last_y = out.clone();
        }
        y[0] = out;
        Ok(())
    }
}

struct Lookup {
    bp: Vec<f64>,
    tb: Vec<f64>,
}
impl Block for Lookup {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let (bp, tb) = (&self.bp, &self.tb);
        let n = bp.len();
        y[0] = map(first(u), |x| {
            let k = if x <= bp[0] {
                0
            } else if x >= bp[n - 1] {
                n - 2
            } else {
                bp.iter().rposition(|b| *b <= x).unwrap_or(0).min(n - 2)
            };
            let f = (x - bp[k]) / (bp[k + 1] - bp[k]);
            tb[k] + f * (tb[k + 1] - tb[k])
        });
        Ok(())
    }
}

// ---------------------------------------------------------------- routing

struct Mux {
    n: usize,
}
impl Block for Mux {
    fn ports(&self) -> (usize, usize) {
        (self.n, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = u.iter().flatten().copied().collect();
        Ok(())
    }
}

/// Copies the hidden element inputs the engine wired from the bus's creator.
struct BusSelect {
    pick: Vec<usize>,
    n_in: usize,
}
impl Block for BusSelect {
    fn ports(&self) -> (usize, usize) {
        (self.n_in, self.pick.len())
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        for (k, &e) in self.pick.iter().enumerate() {
            y[k] = u[1 + e].clone();
        }
        Ok(())
    }
}

struct Demux {
    n: usize,
}
impl Block for Demux {
    fn ports(&self) -> (usize, usize) {
        (1, self.n)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let u = first(u);
        if u.len() == self.n {
            for (i, out) in y.iter_mut().enumerate() {
                *out = vec![u[i]];
            }
        } else {
            // Split as evenly as possible.
            let per = u.len().div_ceil(self.n.max(1)).max(1);
            for (i, out) in y.iter_mut().enumerate() {
                *out = u.iter().skip(i * per).take(per).copied().collect();
            }
        }
        Ok(())
    }
}

struct Switch {
    mode: String,
    th: f64,
}
impl Block for Switch {
    fn ports(&self) -> (usize, usize) {
        (3, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let c = u.get(1).and_then(|v| v.first()).copied().unwrap_or(0.0);
        let pass = match self.mode.as_str() {
            "u2 > threshold" => c > self.th,
            "u2 ~= 0" => c != 0.0,
            _ => c >= self.th,
        };
        y[0] = if pass { u[0].clone() } else { u[2].clone() };
        Ok(())
    }
    fn zero_crossings(&self, _: f64, _: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        let c = u.get(1).and_then(|v| v.first()).copied().unwrap_or(0.0);
        vec![c - if self.mode == "u2 ~= 0" { 0.0 } else { self.th }]
    }
}

struct Rel {
    op: String,
}
impl Block for Rel {
    fn ports(&self) -> (usize, usize) {
        (2, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let op = self.op.clone();
        y[0] = zip2(&u[0], &u[1], |a, b| {
            let r = match op.as_str() {
                "==" => a == b,
                "~=" => a != b,
                "<" => a < b,
                ">" => a > b,
                ">=" => a >= b,
                _ => a <= b,
            };
            r as i32 as f64
        });
        Ok(())
    }
    fn zero_crossings(&self, _: f64, _: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        zip2(&u[0], &u[1], |a, b| a - b)
    }
}

struct Logic {
    op: String,
    n: usize,
}
impl Block for Logic {
    fn ports(&self) -> (usize, usize) {
        (self.n, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let b = |x: f64| x != 0.0;
        if self.op == "NOT" {
            y[0] = map(first(u), |x| (!b(x)) as i32 as f64);
            return Ok(());
        }
        let mut acc: Vec<f64> = u[0].iter().map(|x| b(*x) as i32 as f64).collect();
        for v in &u[1..] {
            acc = zip2(&acc, v, |a, x| {
                let (a, x) = (a != 0.0, b(x));
                (match self.op.as_str() {
                    "OR" | "NOR" => a || x,
                    "XOR" => a ^ x,
                    _ => a && x,
                }) as i32 as f64
            });
        }
        if self.op == "NAND" || self.op == "NOR" {
            acc = map(&acc, |x| (x == 0.0) as i32 as f64);
        }
        y[0] = acc;
        Ok(())
    }
}

struct MultiSwitch {
    n: usize,
}
impl Block for MultiSwitch {
    fn ports(&self) -> (usize, usize) {
        (self.n + 1, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        let k = first(u)
            .first()
            .copied()
            .unwrap_or(1.0)
            .round()
            .clamp(1.0, self.n as f64) as usize;
        y[0] = u[k].clone();
        Ok(())
    }
    fn zero_crossings(&self, _: f64, _: &[f64], u: &[Vec<f64>]) -> Vec<f64> {
        // Each half-integer of the control signal is a switching point.
        let c = first(u).first().copied().unwrap_or(1.0);
        (1..self.n).map(|k| c - (k as f64 + 0.5)).collect()
    }
}

/// Identity (Inport, Goto/From and subsystem boundaries after flattening).
struct Pass;
impl Block for Pass {
    fn ports(&self) -> (usize, usize) {
        (1, 1)
    }
    fn output(
        &mut self,
        _: &mut Ctx,
        _: f64,
        _: &[f64],
        u: &[Vec<f64>],
        y: &mut [Vec<f64>],
    ) -> SimResult<()> {
        y[0] = first(u).to_vec();
        Ok(())
    }
}
