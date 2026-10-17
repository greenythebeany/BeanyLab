//! Block-diagram simulator: model format, block library and simulation engine.

pub mod blocks;
pub mod export;
pub mod import;
pub mod model;
pub mod ode;
pub mod run;
mod solver;

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::time::Instant;

use blocks::{Block, Ctx, LogKind, Params, Sample, SimResult};
pub use model::{BlockSpec, Line, Model, PortRef, SolverSettings};

use crate::interp::Interp;
use crate::value::Value;

/// One block after subsystems are flattened.
struct FlatBlock {
    /// Unique path id, e.g. "sub1/b3".
    id: String,
    /// Display path, e.g. "Controller/Gain".
    name: String,
    kind: String,
    params: BTreeMap<String, String>,
    /// Inside an enabled subsystem: the flat block carrying its enable signal.
    enable: Option<usize>,
    /// Variables of enclosing masked subsystems, outermost first: (name, expression).
    mask: Vec<(String, String)>,
}

struct Flat {
    blocks: Vec<FlatBlock>,
    /// (src block, src port 0-based) -> (dst block, dst port 0-based)
    lines: Vec<((usize, usize), (usize, usize))>,
}

pub fn is_subsystem(kind: &str) -> bool {
    kind == "Subsystem" || kind == "EnabledSubsystem"
}

/// Connect every From block to the Goto block with the same tag. A From without a
/// Goto reads 0 and is reported in `warnings`.
fn wire_goto_from(flat: &mut Flat, warnings: &mut Vec<String>) -> Result<(), SimError> {
    let tag = |b: &FlatBlock| b.params.get("tag").cloned().unwrap_or_else(|| "A".into());
    let mut gotos: BTreeMap<String, usize> = BTreeMap::new();
    for (i, b) in flat.blocks.iter().enumerate() {
        if b.kind == "Goto" && gotos.insert(tag(b), i).is_some() {
            return Err(SimError {
                message: format!(
                    "{}: another Goto block already uses tag '{}'.",
                    b.name,
                    tag(b)
                ),
                block: Some(b.id.clone()),
            });
        }
    }
    for (i, b) in flat.blocks.iter().enumerate() {
        if b.kind == "From" {
            match gotos.get(&tag(b)) {
                Some(&g) => flat.lines.push(((g, 0), (i, 0))),
                None => warnings.push(format!(
                    "{}: no Goto block has tag '{}'; the From block reads 0.",
                    b.name,
                    tag(b)
                )),
            }
        }
    }
    Ok(())
}

/// Give each Bus Selector a hidden input per element of the bus it reads, wired
/// to the signal that feeds that element at the Bus Creator.
fn wire_buses(flat: &mut Flat) -> Result<(), SimError> {
    let feeder = |flat: &Flat, b: usize, p: usize| {
        flat.lines
            .iter()
            .find(|(_, d)| *d == (b, p))
            .map(|(s, _)| *s)
    };
    for i in 0..flat.blocks.len() {
        if flat.blocks[i].kind != "BusSelector" {
            continue;
        }
        let err = |m: String| SimError {
            message: format!("{}: {}", flat.blocks[i].name, m),
            block: Some(flat.blocks[i].id.clone()),
        };
        // Follow pass-through blocks (ports, Goto/From) back to the Bus Creator.
        let mut src = feeder(flat, i, 0).ok_or_else(|| err("input is not connected.".into()))?;
        for _ in 0..1000 {
            let k = flat.blocks[src.0].kind.as_str();
            if k == "BusCreator" {
                break;
            }
            if !matches!(k, "Pass" | "Inport" | "Outport" | "Goto" | "From") {
                return Err(err("input is not a bus from a Bus Creator.".into()));
            }
            src =
                feeder(flat, src.0, 0).ok_or_else(|| err("bus source is not connected.".into()))?;
        }
        let creator = src.0;
        let names = blocks::bus_names(
            flat.blocks[creator]
                .params
                .get("names")
                .map(String::as_str)
                .unwrap_or("signal1, signal2"),
        );
        let select = blocks::bus_names(
            flat.blocks[i]
                .params
                .get("select")
                .map(String::as_str)
                .unwrap_or("signal1"),
        );
        let mut pick = Vec::new();
        for s in &select {
            let e = names
                .iter()
                .position(|n| n == s)
                .ok_or_else(|| err(format!("the bus has no element '{}'.", s)))?;
            pick.push(e.to_string());
        }
        for e in 0..names.len() {
            if let Some(s) = feeder(flat, creator, e) {
                flat.lines.push((s, (i, 1 + e)));
            }
        }
        let p = &mut flat.blocks[i].params;
        p.insert("_elements".into(), names.len().to_string());
        p.insert("_pick".into(), pick.join(","));
    }
    Ok(())
}

/// Inport/Outport counts of a subsystem's contents.
pub fn subsystem_ports(m: &Model) -> (usize, usize) {
    let count = |k: &str| m.blocks.iter().filter(|b| b.kind == k).count();
    (count("Inport"), count("Outport"))
}

fn port_number(b: &BlockSpec) -> usize {
    b.params
        .get("port")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(1)
}

/// Flatten a model. Returns, for the given level, a map from block id to how its ports
/// resolve: (input port k -> flat index of the block that receives it, output port k -> flat index).
fn flatten(
    m: &Model,
    id_prefix: &str,
    name_prefix: &str,
    top: bool,
    enable: Option<usize>,
    mask: &[(String, String)],
    flat: &mut Flat,
) -> SimResult<BTreeMap<String, (Vec<usize>, Vec<usize>)>> {
    let mut ports: BTreeMap<String, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for b in &m.blocks {
        let id = format!("{}{}", id_prefix, b.id);
        let name = format!(
            "{}{}",
            name_prefix,
            if b.name.is_empty() { &b.kind } else { &b.name }
        );
        if is_subsystem(&b.kind) {
            // An enabled subsystem routes its enable input through a pass block that
            // every block inside consults.
            let inner_enable = if b.kind == "EnabledSubsystem" {
                let idx = flat.blocks.len();
                flat.blocks.push(FlatBlock {
                    id: format!("{}/__enable", id),
                    name: format!("{}/enable", name),
                    kind: "Pass".into(),
                    params: BTreeMap::new(),
                    enable,
                    mask: mask.to_vec(),
                });
                Some(idx)
            } else {
                enable
            };
            let sys = b.system.as_deref().cloned().unwrap_or(Model {
                version: 1,
                name: String::new(),
                solver: Default::default(),
                blocks: vec![],
                lines: vec![],
            });
            // Mask parameters ("mask:<variable>") are visible to every block inside.
            let mut inner_mask = mask.to_vec();
            for (k, v) in &b.params {
                if let Some(var) = k.strip_prefix("mask:") {
                    inner_mask.push((var.to_string(), v.clone()));
                }
            }
            let inner = flatten(
                &sys,
                &format!("{}/", id),
                &format!("{}/", name),
                false,
                inner_enable,
                &inner_mask,
                flat,
            )?;
            // Subsystem port k maps to the Inport/Outport block with port number k inside.
            let mut ins: Vec<(usize, usize)> = Vec::new();
            let mut outs: Vec<(usize, usize)> = Vec::new();
            for ib in &sys.blocks {
                if let Some((_, o)) = inner.get(&ib.id) {
                    if ib.kind == "Inport" {
                        ins.push((port_number(ib), o[0]));
                    } else if ib.kind == "Outport" {
                        outs.push((port_number(ib), o[0]));
                    }
                }
            }
            ins.sort();
            outs.sort();
            let mut in_idx: Vec<usize> = ins.into_iter().map(|p| p.1).collect();
            if b.kind == "EnabledSubsystem" {
                in_idx.push(inner_enable.unwrap());
            }
            ports.insert(
                b.id.clone(),
                (in_idx, outs.into_iter().map(|p| p.1).collect()),
            );
            continue;
        }
        let kind = match b.kind.as_str() {
            // Inside a subsystem, ports are pass-throughs; at the top level an Outport logs.
            "Inport" => "Inport".to_string(),
            "Outport" if !top => "Pass".to_string(),
            k => k.to_string(),
        };
        let idx = flat.blocks.len();
        flat.blocks.push(FlatBlock {
            id,
            name,
            kind,
            params: b.params.clone(),
            enable,
            mask: mask.to_vec(),
        });
        ports.insert(b.id.clone(), (vec![idx], vec![idx]));
    }
    for l in &m.lines {
        let src = ports
            .get(&l.src.block)
            .ok_or_else(|| format!("A line starts at missing block '{}'.", l.src.block))?;
        let dst = ports
            .get(&l.dst.block)
            .ok_or_else(|| format!("A line ends at missing block '{}'.", l.dst.block))?;
        let src_is_sub = m
            .blocks
            .iter()
            .any(|b| b.id == l.src.block && is_subsystem(&b.kind));
        let dst_is_sub = m
            .blocks
            .iter()
            .any(|b| b.id == l.dst.block && is_subsystem(&b.kind));
        let (s, sp) = if src_is_sub {
            (
                *src.1
                    .get(l.src.port.saturating_sub(1))
                    .ok_or("A line uses a missing subsystem output port.")?,
                0,
            )
        } else {
            (src.1[0], l.src.port.saturating_sub(1))
        };
        let (d, dp) = if dst_is_sub {
            (
                *dst.0
                    .get(l.dst.port.saturating_sub(1))
                    .ok_or("A line uses a missing subsystem input port.")?,
                0,
            )
        } else {
            (dst.0[0], l.dst.port.saturating_sub(1))
        };
        flat.lines.push(((s, sp), (d, dp)));
    }
    Ok(ports)
}

pub struct Log {
    pub id: String,
    pub name: String,
    pub kind: LogKind,
    /// Per input port: one row of values per logged time.
    pub data: Vec<Vec<Vec<f64>>>,
}

pub struct SimOutput {
    pub tout: Vec<f64>,
    pub logs: Vec<Log>,
    pub warnings: Vec<String>,
    pub stopped_by_user: bool,
}

/// Simulation failure tied to a block when possible, so the editor can highlight it.
#[derive(Debug)]
pub struct SimError {
    pub message: String,
    pub block: Option<String>,
}

impl From<String> for SimError {
    fn from(message: String) -> Self {
        SimError {
            message,
            block: None,
        }
    }
}

struct Engine {
    blocks: Vec<Box<dyn Block>>,
    ids: Vec<String>,
    names: Vec<String>,
    inputs: Vec<Vec<Option<(usize, usize)>>>,
    outputs: Vec<Vec<Vec<f64>>>,
    order: Vec<usize>,
    xoff: Vec<usize>,
    nx: usize,
    samples: Vec<Sample>,
    constant_done: Vec<bool>,
    zc_map: Vec<(usize, usize)>,
    /// Enable-signal block for blocks inside enabled subsystems.
    enable: Vec<Option<usize>>,
}

const EPS: f64 = 1e-10;

fn is_hit(s: Sample, t: f64) -> bool {
    match s {
        Sample::Discrete { period, offset } if period > 0.0 => {
            let k = ((t - offset) / period).round();
            k >= 0.0 && (t - offset - k * period).abs() <= EPS * period.max(1.0) * 10.0
        }
        _ => false,
    }
}

fn next_hit(s: Sample, t: f64) -> Option<f64> {
    match s {
        Sample::Discrete { period, offset } if period > 0.0 => {
            let k = ((t - offset) / period + 1e-9).floor() + 1.0;
            Some(offset + k.max(0.0) * period)
        }
        _ => None,
    }
}

impl Engine {
    /// False while an enclosing enabled subsystem's enable input is not positive.
    fn enabled(&self, b: usize) -> bool {
        match self.enable[b] {
            None => true,
            Some(e) => {
                self.outputs[e]
                    .first()
                    .and_then(|v| v.first())
                    .is_some_and(|x| *x > 0.0)
                    && self.enabled(e)
            }
        }
    }

    fn inputs_of(&self, b: usize) -> Vec<Vec<f64>> {
        self.inputs[b]
            .iter()
            .map(|src| match src {
                Some((s, p)) => self.outputs[*s]
                    .get(*p)
                    .cloned()
                    .unwrap_or_else(|| vec![0.0]),
                None => vec![0.0],
            })
            .collect()
    }

    fn compute_outputs(
        &mut self,
        it: &mut Interp,
        t: f64,
        x: &[f64],
        major: bool,
    ) -> Result<(), SimError> {
        for k in 0..self.order.len() {
            let b = self.order[k];
            match self.samples[b] {
                Sample::Discrete { .. } if !(major && is_hit(self.samples[b], t)) => continue,
                Sample::Constant if self.constant_done[b] => continue,
                _ => {}
            }
            // Disabled blocks hold their last output.
            if !self.enabled(b) {
                continue;
            }
            let u = self.inputs_of(b);
            let xs = &x[self.xoff[b]..self.xoff[b] + self.blocks[b].nstates()];
            let mut y = std::mem::take(&mut self.outputs[b]);
            let mut ctx = Ctx { interp: it, major };
            self.blocks[b]
                .output(&mut ctx, t, xs, &u, &mut y)
                .map_err(|m| SimError {
                    message: if m.starts_with(&self.names[b]) {
                        m
                    } else {
                        format!("{}: {}", self.names[b], m)
                    },
                    block: Some(self.ids[b].clone()),
                })?;
            self.outputs[b] = y;
            if self.samples[b] == Sample::Constant {
                self.constant_done[b] = true;
            }
        }
        Ok(())
    }

    fn derivatives(&mut self, it: &mut Interp, t: f64, x: &[f64]) -> Result<Vec<f64>, SimError> {
        self.compute_outputs(it, t, x, false)?;
        let mut dx = vec![0.0; self.nx];
        for b in 0..self.blocks.len() {
            let n = self.blocks[b].nstates();
            // Disabled blocks keep their states frozen.
            if n == 0 || !self.enabled(b) {
                continue;
            }
            let u = self.inputs_of(b);
            let o = self.xoff[b];
            self.blocks[b].derivative(t, &x[o..o + n], &u, &mut dx[o..o + n]);
        }
        Ok(dx)
    }

    fn zero_crossings(&mut self, t: f64, x: &[f64]) -> Vec<f64> {
        let mut z = Vec::new();
        self.zc_map.clear();
        for b in 0..self.blocks.len() {
            let u = self.inputs_of(b);
            let n = self.blocks[b].nstates();
            let o = self.xoff[b];
            for (i, v) in self.blocks[b]
                .zero_crossings(t, &x[o..o + n], &u)
                .into_iter()
                .enumerate()
            {
                z.push(v);
                self.zc_map.push((b, i));
            }
        }
        z
    }

    fn project(&self, x: &mut [f64]) {
        for b in 0..self.blocks.len() {
            let n = self.blocks[b].nstates();
            if n > 0 {
                let o = self.xoff[b];
                self.blocks[b].project(&mut x[o..o + n]);
            }
        }
    }
}

/// Nodes in strongly connected components that contain a cycle (Tarjan's algorithm).
fn cycle_members(succ: &[Vec<usize>], n: usize) -> Vec<usize> {
    struct T<'a> {
        succ: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<usize>,
    }
    fn visit(t: &mut T, v: usize) {
        t.index[v] = Some(t.next);
        t.low[v] = t.next;
        t.next += 1;
        t.stack.push(v);
        t.on[v] = true;
        for &w in &t.succ[v] {
            match t.index[w] {
                None => {
                    visit(t, w);
                    t.low[v] = t.low[v].min(t.low[w]);
                }
                Some(iw) if t.on[w] => t.low[v] = t.low[v].min(iw),
                _ => {}
            }
        }
        if Some(t.low[v]) == t.index[v] {
            let mut comp = Vec::new();
            while let Some(w) = t.stack.pop() {
                t.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            if comp.len() > 1 || t.succ[v].contains(&v) {
                t.out.extend(comp);
            }
        }
    }
    let mut t = T {
        succ,
        index: vec![None; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if t.index[v].is_none() {
            visit(&mut t, v);
        }
    }
    t.out.sort();
    t.out
}

fn crossed(z0: f64, z1: f64) -> Option<bool> {
    if z0 < 0.0 && z1 >= 0.0 {
        Some(true)
    } else if z0 > 0.0 && z1 <= 0.0 {
        Some(false)
    } else {
        None
    }
}

fn eval_param(it: &mut Interp, block: &str, name: &str, src: &str) -> SimResult<Value> {
    let e = crate::parser::parse_expression(src)
        .map_err(|e| format!("{}: parameter '{}' = '{}': {}", block, name, src, e.message))?;
    it.eval(&e)
        .map_err(|e| format!("{}: parameter '{}' = '{}': {}", block, name, src, e.message))
}

fn eval_setting(it: &mut Interp, what: &str, src: &str) -> SimResult<Option<f64>> {
    let s = src.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let v = eval_param(it, "Solver settings", what, s)?;
    v.scalar().map(Some).map_err(|e| e.message)
}

/// Run a model to completion. `progress` is called about every 100 ms with the data so far.
pub fn simulate(
    model: &Model,
    it: &mut Interp,
    mut progress: impl FnMut(&mut Interp, &[f64], &[Log]),
) -> Result<SimOutput, SimError> {
    let mut flat = Flat {
        blocks: Vec::new(),
        lines: Vec::new(),
    };
    flatten(model, "", "", true, None, &[], &mut flat)?;
    let mut warnings = Vec::new();
    wire_goto_from(&mut flat, &mut warnings)?;
    wire_buses(&mut flat)?;
    let defs = blocks::library();

    // Build runtime blocks with evaluated parameters.
    let mut rblocks: Vec<Box<dyn Block>> = Vec::with_capacity(flat.blocks.len());
    // Sample-time offsets from `ts = [period offset]`.
    let mut offsets: Vec<f64> = vec![0.0; flat.blocks.len()];
    for (bi, fb) in flat.blocks.iter().enumerate() {
        let def_kind = if fb.kind == "Pass" {
            "Inport"
        } else {
            fb.kind.as_str()
        };
        let def = defs.iter().find(|d| d.kind == def_kind);
        let mut values = BTreeMap::new();
        let mut texts = BTreeMap::new();
        // Masked subsystems: their variables exist while this block's parameters are evaluated.
        let saved: Vec<(String, Option<Value>)> = fb
            .mask
            .iter()
            .map(|(n, _)| (n.clone(), it.get_var(n).cloned()))
            .collect();
        let mut evaluate = |it: &mut Interp| -> Result<(), SimError> {
            for (n, e) in &fb.mask {
                let v = eval_param(it, &fb.name, n, e).map_err(|m| SimError {
                    message: m,
                    block: Some(fb.id.clone()),
                })?;
                it.set_var(n, v);
            }
            if let Some(def) = def {
                for pd in &def.params {
                    let raw = fb
                        .params
                        .get(pd.name)
                        .cloned()
                        .unwrap_or_else(|| pd.default.to_string());
                    if pd.kind == "expr" {
                        let v = eval_param(it, &fb.name, pd.name, &raw).map_err(|m| SimError {
                            message: m,
                            block: Some(fb.id.clone()),
                        })?;
                        let v = match (pd.name, v.to_num()) {
                            ("ts", Ok(a)) if a.data.len() == 2 => {
                                let (period, offset) = (a.data[0], a.data[1]);
                                if !(0.0..period).contains(&offset) {
                                    return Err(SimError {
                                        message: format!(
                                            "{}: the sample time offset must be at least 0 and less than the period.",
                                            fb.name
                                        ),
                                        block: Some(fb.id.clone()),
                                    });
                                }
                                offsets[bi] = offset;
                                Value::num(period)
                            }
                            (_, _) => v,
                        };
                        values.insert(pd.name.to_string(), v);
                    } else {
                        texts.insert(pd.name.to_string(), raw);
                    }
                }
            }
            Ok(())
        };
        let evaluated = evaluate(it);
        for (n, v) in saved.into_iter().rev() {
            match v {
                Some(v) => it.set_var(&n, v),
                None => it.clear_var(&n),
            }
        }
        evaluated?;
        // Hidden parameters set by the engine itself.
        for (k, v) in &fb.params {
            if k.starts_with('_') {
                texts.insert(k.clone(), v.clone());
            }
        }
        let params = Params::new(fb.name.clone(), values, texts);
        let ports = blocks::ports_for(&fb.kind, &fb.params, None);
        let kind = if fb.kind == "Inport" || fb.kind == "Pass" {
            "Pass"
        } else {
            fb.kind.as_str()
        };
        let b = blocks::create(kind, &params, ports).map_err(|m| SimError {
            message: m,
            block: Some(fb.id.clone()),
        })?;
        rblocks.push(b);
    }

    let n = rblocks.len();
    let mut inputs: Vec<Vec<Option<(usize, usize)>>> =
        rblocks.iter().map(|b| vec![None; b.ports().0]).collect();
    for ((s, sp), (d, dp)) in &flat.lines {
        if *dp >= inputs[*d].len() {
            return Err(SimError {
                message: format!(
                    "{}: input port {} does not exist.",
                    flat.blocks[*d].name,
                    dp + 1
                ),
                block: Some(flat.blocks[*d].id.clone()),
            });
        }
        if *sp >= rblocks[*s].ports().1 {
            return Err(SimError {
                message: format!(
                    "{}: output port {} does not exist.",
                    flat.blocks[*s].name,
                    sp + 1
                ),
                block: Some(flat.blocks[*s].id.clone()),
            });
        }
        if inputs[*d][*dp].is_some() {
            return Err(SimError {
                message: format!(
                    "{}: input port {} has more than one line connected.",
                    flat.blocks[*d].name,
                    dp + 1
                ),
                block: Some(flat.blocks[*d].id.clone()),
            });
        }
        inputs[*d][*dp] = Some((*s, *sp));
    }
    for (b, ins) in inputs.iter().enumerate() {
        for (p, src) in ins.iter().enumerate() {
            if src.is_none() && flat.blocks[b].kind != "Inport" && flat.blocks[b].kind != "Pass" {
                warnings.push(format!(
                    "{}: input port {} is not connected; it reads 0.",
                    flat.blocks[b].name,
                    p + 1
                ));
            }
        }
    }

    // Execution order: a block runs after every source that feeds it directly.
    let mut indeg = vec![0usize; n];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (d, ins) in inputs.iter().enumerate() {
        for (p, src) in ins.iter().enumerate() {
            if let Some((s, _)) = src {
                if rblocks[d].feedthrough(p) {
                    indeg[d] += 1;
                    succ[*s].push(d);
                }
            }
        }
    }
    // Blocks in an enabled subsystem run after their enable signal is known.
    for (b, fb) in flat.blocks.iter().enumerate() {
        if let Some(e) = fb.enable {
            indeg[b] += 1;
            succ[e].push(b);
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut ready: Vec<usize> = (0..n).filter(|b| indeg[*b] == 0).collect();
    ready.reverse();
    while let Some(b) = ready.pop() {
        order.push(b);
        for &s in &succ[b] {
            indeg[s] -= 1;
            if indeg[s] == 0 {
                ready.push(s);
            }
        }
    }
    if order.len() < n {
        // Report only blocks that are on a cycle, not ones merely downstream of it.
        let cyc = cycle_members(&succ, n);
        let in_loop: Vec<String> = cyc.iter().map(|b| flat.blocks[*b].name.clone()).collect();
        let first = cyc.first().map(|b| flat.blocks[*b].id.clone());
        return Err(SimError {
            message: format!(
                "Algebraic loop: these blocks feed each other with no delay or integrator in between: {}. Break the loop with an Integrator, Unit Delay or Transport Delay block.",
                in_loop.join(", ")
            ),
            block: first,
        });
    }

    let mut xoff = vec![0; n];
    let mut nx = 0;
    for b in 0..n {
        xoff[b] = nx;
        nx += rblocks[b].nstates();
    }
    let samples: Vec<Sample> = rblocks
        .iter()
        .zip(&offsets)
        .map(|(b, &o)| match b.sample() {
            Sample::Discrete { period, offset } => Sample::Discrete {
                period,
                offset: offset + o,
            },
            s => s,
        })
        .collect();
    let mut eng = Engine {
        outputs: rblocks
            .iter()
            .map(|b| vec![vec![0.0]; b.ports().1])
            .collect(),
        blocks: rblocks,
        ids: flat.blocks.iter().map(|b| b.id.clone()).collect(),
        names: flat.blocks.iter().map(|b| b.name.clone()).collect(),
        inputs,
        order,
        xoff,
        nx,
        samples,
        constant_done: vec![false; n],
        zc_map: Vec::new(),
        enable: flat.blocks.iter().map(|b| b.enable).collect(),
    };

    // Solver settings.
    let s = &model.solver;
    let t_end = eval_setting(it, "Stop time", &s.stop_time)?.unwrap_or(10.0);
    if t_end.is_nan() || t_end < 0.0 {
        return Err("Stop time must be a nonnegative number (or inf)."
            .to_string()
            .into());
    }
    let span = if t_end.is_finite() { t_end } else { 10.0 };
    let max_step = eval_setting(it, "Max step size", &s.max_step)?
        .unwrap_or(span / 200.0)
        .max(1e-12);
    let rel_tol = eval_setting(it, "Relative tolerance", &s.rel_tol)?.unwrap_or(1e-3);
    let abs_tol = eval_setting(it, "Absolute tolerance", &s.abs_tol)?.unwrap_or(1e-6);
    let min_period = eng
        .samples
        .iter()
        .filter_map(|s| {
            if let Sample::Discrete { period, .. } = s {
                Some(*period)
            } else {
                None
            }
        })
        .fold(f64::INFINITY, f64::min);
    let fixed_step = eval_setting(it, "Fixed step size", &s.fixed_step)?
        .unwrap_or(if min_period.is_finite() {
            min_period.min(span / 50.0)
        } else {
            span / 200.0
        })
        .max(1e-12);
    let method = match s.solver.as_str() {
        "auto" | "" => {
            if eng.nx > 0 {
                solver::Method::Ode45
            } else {
                solver::Method::Discrete
            }
        }
        other => {
            solver::Method::parse(other).ok_or_else(|| format!("Unknown solver '{}'.", other))?
        }
    };

    // Initial states and outputs.
    let mut x = vec![0.0; eng.nx];
    for b in 0..n {
        let k = eng.blocks[b].nstates();
        let o = eng.xoff[b];
        eng.blocks[b].init(&mut x[o..o + k]);
    }
    let mut t = 0.0;
    eng.compute_outputs(it, t, &x, true)?;

    let mut logs: Vec<Log> = Vec::new();
    let mut log_blocks = Vec::new();
    for b in 0..n {
        if let Some(kind) = eng.blocks[b].log() {
            logs.push(Log {
                id: eng.ids[b].clone(),
                name: eng.names[b].clone(),
                kind,
                data: vec![Vec::new(); eng.blocks[b].ports().0],
            });
            log_blocks.push(b);
        }
    }
    let mut tout = Vec::new();
    let mut h = solver::initial_step(max_step, span);
    let started = Instant::now();
    let mut last_progress = Instant::now();
    let mut stopped_by_user = false;
    let interrupt = it.interrupt.clone();
    let pause = it.pause.clone();
    pause.store(false, Ordering::Relaxed);

    loop {
        // Log this major step.
        tout.push(t);
        for (li, &b) in log_blocks.iter().enumerate() {
            let u = eng.inputs_of(b);
            for (p, v) in u.into_iter().enumerate() {
                logs[li].data[p].push(v);
            }
        }
        if (0..n).any(|b| eng.blocks[b].stops(&eng.inputs_of(b))) {
            break;
        }
        if t >= t_end - EPS * t_end.abs().max(1.0) {
            break;
        }
        if interrupt.load(Ordering::Relaxed) {
            interrupt.store(false, Ordering::Relaxed);
            stopped_by_user = true;
            break;
        }
        if last_progress.elapsed().as_millis() >= 100 {
            progress(it, &tout, &logs);
            last_progress = Instant::now();
        }
        if pause.load(Ordering::Relaxed) {
            progress(it, &tout, &logs);
            it.emit("sim-state", serde_json::json!({ "paused": true, "t": t }));
            while pause.load(Ordering::Relaxed) && !interrupt.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            pause.store(false, Ordering::Relaxed);
            it.emit("sim-state", serde_json::json!({ "paused": false, "t": t }));
            if interrupt.swap(false, Ordering::Relaxed) {
                stopped_by_user = true;
                break;
            }
        }
        if !t_end.is_finite() && started.elapsed().as_secs() > 3600 {
            warnings
                .push("Simulation with infinite stop time ran for an hour and was stopped.".into());
            break;
        }

        // Discrete and per-step updates at this instant.
        for b in 0..n {
            let upd = match eng.samples[b] {
                Sample::Discrete { .. } => is_hit(eng.samples[b], t),
                Sample::Continuous => true,
                Sample::Constant => false,
            };
            if upd && eng.enabled(b) {
                let u = eng.inputs_of(b);
                let k = eng.blocks[b].nstates();
                let o = eng.xoff[b];
                eng.blocks[b].update(t, &mut x[o..o + k], &u);
            }
        }
        let z0 = eng.zero_crossings(t, &x);

        // Step size bounded by the next sample hit and the stop time.
        let next_disc = eng
            .samples
            .iter()
            .filter_map(|s| next_hit(*s, t))
            .fold(f64::INFINITY, f64::min);
        let limit = (next_disc - t).min(t_end - t).max(0.0);
        let (x1, h_used) = match method {
            solver::Method::Discrete => {
                let hh = limit.min(max_step);
                (x.clone(), hh)
            }
            m if m.is_fixed() => {
                let hh = fixed_step.min(limit);
                (solver::fixed_step(m, &mut eng, it, t, &x, hh)?, hh)
            }
            m => {
                let (x1, hh, next_h) = solver::adaptive_step(
                    m,
                    &mut eng,
                    it,
                    t,
                    &x,
                    h.min(limit).min(max_step),
                    limit.min(max_step),
                    rel_tol,
                    abs_tol,
                )?;
                h = next_h;
                (x1, hh)
            }
        };
        if h_used <= 0.0 {
            break;
        }
        let mut t1 = t + h_used;
        let mut x1 = x1;
        eng.project(&mut x1);

        // Locate zero crossings inside the step by bisection.
        eng.compute_outputs(it, t1, &x1, false)?;
        let z1 = eng.zero_crossings(t1, &x1);
        let mut events: Vec<(usize, usize, bool)> = Vec::new();
        if z0.len() == z1.len() && z0.iter().zip(&z1).any(|(a, b)| crossed(*a, *b).is_some()) {
            let (mut lo, mut hi) = (0.0, h_used);
            let mut x_hi = x1.clone();
            for _ in 0..60 {
                if hi - lo <= 1e-12 * t1.abs().max(1.0) {
                    break;
                }
                let mid = 0.5 * (lo + hi);
                let xm = solver::step_to(method, &mut eng, it, t, &x, mid)?;
                eng.compute_outputs(it, t + mid, &xm, false)?;
                let zm = eng.zero_crossings(t + mid, &xm);
                if z0.iter().zip(&zm).any(|(a, b)| crossed(*a, *b).is_some()) {
                    hi = mid;
                    x_hi = xm;
                } else {
                    lo = mid;
                }
            }
            t1 = t + hi;
            x1 = x_hi;
            eng.project(&mut x1);
            eng.compute_outputs(it, t1, &x1, false)?;
            let zh = eng.zero_crossings(t1, &x1);
            for (i, (a, b)) in z0.iter().zip(&zh).enumerate() {
                if let Some(rising) = crossed(*a, *b) {
                    let (blk, which) = eng.zc_map[i];
                    events.push((blk, which, rising));
                }
            }
        }

        t = t1;
        x = x1;
        eng.compute_outputs(it, t, &x, true)?;
        if !events.is_empty() {
            for (b, which, rising) in events {
                let u = eng.inputs_of(b);
                let k = eng.blocks[b].nstates();
                let o = eng.xoff[b];
                eng.blocks[b].event(t, &mut x[o..o + k], &u, which, rising);
            }
            eng.compute_outputs(it, t, &x, true)?;
            // Restart the adaptive step size after a discontinuity.
            h = solver::initial_step(max_step, span);
        }
    }
    progress(it, &tout, &logs);
    Ok(SimOutput {
        tout,
        logs,
        warnings,
        stopped_by_user,
    })
}

impl solver::System for Engine {
    fn nx(&self) -> usize {
        self.nx
    }
    fn f(&mut self, it: &mut Interp, t: f64, x: &[f64]) -> Result<Vec<f64>, SimError> {
        self.derivatives(it, t, x)
    }
}
