//! Glue between the simulator and the language: `sim`, scopes as figures, `out` struct,
//! and `get_param`/`set_param` on model files.

use std::path::PathBuf;

use serde::Serialize;

use super::blocks::LogKind;
use super::{simulate, Log, Model, SimError};
use crate::error::{MError, MResult};
use crate::graphics::{Axes, LineStyle, Series, DEFAULT_COLORS};
use crate::interp::Interp;
use crate::merr;
use crate::value::{Array, StructArr, Value};

#[derive(Debug, Clone, Serialize)]
pub struct DisplayValue {
    pub id: String,
    pub value: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub displays: Vec<DisplayValue>,
    pub error_block: Option<String>,
    pub stop_time: f64,
}

/// Find `name` or `name.bmdl` in the current folder or on the path.
pub fn resolve_model(it: &Interp, name: &str) -> Option<PathBuf> {
    let with_ext = if name.to_lowercase().ends_with(".bmdl") {
        name.to_string()
    } else {
        format!("{}.bmdl", name)
    };
    let direct = PathBuf::from(&with_ext);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    let here = it.cwd.join(&with_ext);
    if here.is_file() {
        return Some(here);
    }
    it.path
        .iter()
        .map(|d| d.join(&with_ext))
        .find(|p| p.is_file())
}

/// Block type from a library path such as "beany/Continuous/Transfer Fcn" or "Gain".
pub fn block_type_from(src: &str) -> Option<&'static str> {
    let last = src.rsplit('/').next().unwrap_or(src);
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    };
    let want = norm(last);
    let alias = match want.as_str() {
        "in1" | "in" => "Inport",
        "out1" | "out" => "Outport",
        "pidcontroller" => "PID",
        "discretepidcontroller" => "DiscretePID",
        "trigonometricfunction" => "TrigFunction",
        "1dlookuptable" | "lookuptable" => "Lookup1D",
        "matlabfunction" | "interpretedmatlabfunction" => "UserFunction",
        "discretetimeintegrator" => "DiscreteIntegrator",
        "add" | "subtract" => "Sum",
        "multiply" | "divide" => "Product",
        _ => "",
    };
    super::blocks::library()
        .into_iter()
        .map(|d| d.kind)
        .find(|k| *k == alias || norm(k) == want)
}

/// The model's base name: "dir/pid.bmdl" -> "pid".
fn model_key(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.strip_suffix(".bmdl").unwrap_or(base).to_string()
}

/// A model open in memory (from new_system/load_system), else loaded from its file.
pub fn get_model(it: &Interp, name: &str) -> MResult<(Model, Option<PathBuf>)> {
    if let Some((m, p)) = it.open_models.get(&model_key(name)) {
        return Ok((m.clone(), p.clone()));
    }
    let (m, p) = load_model(it, name)?;
    Ok((m, Some(p)))
}

/// Store an edited model: back into memory if it is open there, else to its file.
fn put_model(it: &mut Interp, name: &str, model: Model, file: Option<PathBuf>) -> MResult<()> {
    let key = model_key(name);
    if let Some(slot) = it.open_models.get_mut(&key) {
        slot.0 = model;
        return Ok(());
    }
    let file = file.ok_or_else(|| merr!("Model '{}' has no file; use save_system.", name))?;
    std::fs::write(&file, model.to_json())
        .map_err(|e| merr!("Unable to save '{}': {}", file.display(), e))
}

pub fn load_model(it: &Interp, name: &str) -> MResult<(Model, PathBuf)> {
    let p = resolve_model(it, name).ok_or_else(|| {
        merr!(
            "Model '{}' not found. Model files use the .bmdl extension.",
            name
        )
    })?;
    let text = std::fs::read_to_string(&p)
        .map_err(|e| merr!("Unable to read '{}': {}", p.display(), e))?;
    let model = Model::parse(&text).map_err(MError::new)?;
    Ok((model, p))
}

fn draw_scopes(it: &mut Interp, tout: &[f64], logs: &[Log]) {
    for log in logs {
        let LogKind::Scope { separate } = log.kind else {
            continue;
        };
        let title = log.name.clone();
        let id = match it
            .graphics
            .figures
            .iter()
            .find(|f| f.name.as_deref() == Some(title.as_str()))
        {
            Some(f) => f.id,
            None => {
                let id = it.graphics.figure(None);
                if let Some(f) = it.graphics.figures.iter_mut().find(|f| f.id == id) {
                    f.name = Some(title.clone());
                    f.scope = true;
                }
                id
            }
        };
        // One group of lines per axes: all inputs together, or one input per axes.
        let mut groups: Vec<(Vec<Series>, Vec<String>)> = Vec::new();
        let mut series = Vec::new();
        let mut names = Vec::new();
        let mut color = 0;
        for (p, rows) in log.data.iter().enumerate() {
            if separate && p > 0 {
                groups.push((std::mem::take(&mut series), std::mem::take(&mut names)));
            }
            let width = rows.iter().map(|r| r.len()).max().unwrap_or(1);
            for k in 0..width {
                let y: Vec<f64> = rows
                    .iter()
                    .map(|r| *r.get(k).or(r.first()).unwrap_or(&f64::NAN))
                    .collect();
                let n = y.len().min(tout.len());
                let style = LineStyle {
                    color: Some(DEFAULT_COLORS[color % DEFAULT_COLORS.len()].to_string()),
                    width: Some(1.5),
                    ..Default::default()
                };
                color += 1;
                series.push(Series::Line {
                    x: tout[..n].to_vec(),
                    y: y[..n].to_vec(),
                    z: None,
                    style,
                    name: None,
                });
                names.push(if log.data.len() > 1 || width > 1 {
                    format!(
                        "In{}{}",
                        p + 1,
                        if width > 1 {
                            format!("({})", k + 1)
                        } else {
                            String::new()
                        }
                    )
                } else {
                    "signal".into()
                });
            }
        }
        groups.push((series, names));
        let rows = groups.len();
        let axes: Vec<Axes> = groups
            .into_iter()
            .enumerate()
            .map(|(i, (series, names))| {
                let show_legend = names.len() > 1;
                Axes {
                    grid: (rows, 1),
                    cells: vec![i + 1],
                    series,
                    title: if i == 0 { Some(title.clone()) } else { None },
                    xlabel: if i + 1 == rows {
                        Some("Time (s)".into())
                    } else {
                        None
                    },
                    ylabel: if rows > 1 {
                        Some(format!("In{}", i + 1))
                    } else {
                        None
                    },
                    grid_on: true,
                    box_on: true,
                    legend: if show_legend { Some(names) } else { None },
                    legend_visible: show_legend,
                    ..Default::default()
                }
            })
            .collect();
        if let Some(f) = it.graphics.figures.iter_mut().find(|f| f.id == id) {
            f.axes = axes;
            f.current_axes = 0;
        }
        it.graphics.mark(id);
    }
}

/// Run a model and put its results into the language: `out` struct and scope figures.
/// Returns the `out` value and a report for the editor.
pub fn run(
    it: &mut Interp,
    model: &Model,
    label: &str,
) -> Result<(Value, RunReport), Box<(MError, Option<String>)>> {
    let result = simulate(model, it, |it, tout, logs| {
        draw_scopes(it, tout, logs);
        it.flush_graphics();
        let t = tout.last().copied().unwrap_or(0.0);
        let displays = serde_json::to_value(display_values(logs)).unwrap_or_default();
        it.emit(
            "sim-progress",
            serde_json::json!({ "t": t, "displays": displays }),
        );
    });
    let out = match result {
        Ok(o) => o,
        Err(SimError { message, block }) => {
            let mut e = MError::with_id("Beany:sim", message);
            e.origin = Some(format!("sim ({})", label));
            return Err(Box::new((e, block)));
        }
    };
    for w in &out.warnings {
        it.warn(w);
    }
    if out.stopped_by_user {
        it.warn("Simulation stopped by user.");
    }
    draw_scopes(it, &out.tout, &out.logs);
    let n = out.tout.len();
    let mut fields: Vec<(String, Value)> = vec![("tout".into(), Value::col(out.tout.clone()))];
    let mut outports: Vec<(usize, &Log)> = Vec::new();
    let displays = display_values(&out.logs);
    for log in &out.logs {
        match &log.kind {
            LogKind::ToWorkspace(var) => {
                fields.push((var.clone(), rows_to_matrix(&log.data[0], n)))
            }
            LogKind::Outport(k) => outports.push((*k, log)),
            LogKind::Display | LogKind::Scope { .. } => {}
        }
    }
    if !outports.is_empty() {
        outports.sort_by_key(|p| p.0);
        let cols: Vec<Value> = outports
            .iter()
            .map(|(_, l)| rows_to_matrix(&l.data[0], n))
            .collect();
        fields.push((
            "yout".into(),
            crate::ops::horzcat(cols).unwrap_or_else(|_| Value::empty()),
        ));
    }
    let stop = out.tout.last().copied().unwrap_or(0.0);
    Ok((
        Value::Struct(StructArr::scalar(fields)),
        RunReport {
            displays,
            error_block: None,
            stop_time: stop,
        },
    ))
}

/// Latest value shown by each Display block.
fn display_values(logs: &[Log]) -> Vec<DisplayValue> {
    logs.iter()
        .filter(|l| matches!(l.kind, LogKind::Display))
        .map(|l| DisplayValue {
            id: l.id.clone(),
            value: l.data[0].last().cloned().unwrap_or_default(),
        })
        .collect()
}

fn rows_to_matrix(rows: &[Vec<f64>], n: usize) -> Value {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    let rows = &rows[..rows.len().min(n)];
    let mut a = Array::filled(rows.len(), width, 0.0);
    for (r, row) in rows.iter().enumerate() {
        for c in 0..width {
            a.set(r, c, *row.get(c).or(row.first()).unwrap_or(&0.0));
        }
    }
    Value::mat(a)
}

/// Find a block by "model/Path/To/Block" name inside a loaded model.
fn find_block<'a>(m: &'a mut Model, path: &[&str]) -> Option<&'a mut super::BlockSpec> {
    let (head, rest) = path.split_first()?;
    let b = m
        .blocks
        .iter_mut()
        .find(|b| (if b.name.is_empty() { &b.kind } else { &b.name }) == head || b.id == *head)?;
    if rest.is_empty() {
        return Some(b);
    }
    find_block(b.system.as_deref_mut()?, rest)
}

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("sim", |it, a, nargout| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        let name = a[0].text()?;
        let (mut model, _) = get_model(it, &name)?;
        let mut i = 1;
        while i + 1 < a.len() {
            let key = a[i].text()?;
            let val = match &a[i + 1] {
                v if v.is_text() => v.text()?,
                v => crate::display::num_to_str(v.scalar()?),
            };
            match key.to_lowercase().as_str() {
                "stoptime" => model.solver.stop_time = val,
                "solver" => model.solver.solver = val,
                "fixedstep" => model.solver.fixed_step = val,
                "maxstep" => model.solver.max_step = val,
                "reltol" => model.solver.rel_tol = val,
                "abstol" => model.solver.abs_tol = val,
                other => return Err(merr!("Unknown sim option '{}'.", other)),
            }
            i += 2;
        }
        let label = model_key(&name);
        let (out, _) = run(it, &model, &label).map_err(|e| e.0)?;
        if nargout > 0 {
            return Ok(vec![out]);
        }
        it.set_var("out", out);
        Ok(vec![])
    });
    b.insert("get_param", |it, a, _| {
        if a.len() != 2 {
            return Err(merr!("Usage: get_param('model/Block', 'param')"));
        }
        let path = a[0].text()?;
        let key = a[1].text()?;
        let parts: Vec<&str> = path.split('/').collect();
        let (mut model, _) = get_model(it, parts[0])?;
        if parts.len() == 1 {
            let s = &model.solver;
            let v = match key.to_lowercase().as_str() {
                "stoptime" => s.stop_time.clone(),
                "solver" => s.solver.clone(),
                "fixedstep" => s.fixed_step.clone(),
                "maxstep" => s.max_step.clone(),
                "reltol" => s.rel_tol.clone(),
                "abstol" => s.abs_tol.clone(),
                other => return Err(merr!("Unknown model parameter '{}'.", other)),
            };
            return Ok(vec![Value::chars(&v)]);
        }
        let blk = find_block(&mut model, &parts[1..])
            .ok_or_else(|| merr!("Block '{}' not found.", path))?;
        match key.as_str() {
            "BlockType" | "type" => Ok(vec![Value::chars(&blk.kind)]),
            "Name" | "name" => Ok(vec![Value::chars(&blk.name)]),
            k => {
                let def = super::blocks::library()
                    .into_iter()
                    .find(|d| d.kind == blk.kind);
                let default = def.and_then(|d| {
                    d.params
                        .iter()
                        .find(|p| p.name == k)
                        .map(|p| p.default.to_string())
                });
                let v = blk
                    .params
                    .get(k)
                    .cloned()
                    .or(default)
                    .ok_or_else(|| merr!("Block '{}' has no parameter '{}'.", path, k))?;
                Ok(vec![Value::chars(&v)])
            }
        }
    });
    b.insert("set_param", |it, a, _| {
        if a.len() < 3 || a.len() % 2 == 0 {
            return Err(merr!(
                "Usage: set_param('model/Block', 'param', 'value', ...)"
            ));
        }
        let path = a[0].text()?;
        let parts: Vec<&str> = path.split('/').collect();
        let (mut model, file) = get_model(it, parts[0])?;
        for pair in a[1..].chunks(2) {
            let key = pair[0].text()?;
            let val = match &pair[1] {
                v if v.is_text() => v.text()?,
                v => crate::display::num_to_str(v.scalar()?),
            };
            if parts.len() == 1 {
                let s = &mut model.solver;
                match key.to_lowercase().as_str() {
                    "stoptime" => s.stop_time = val,
                    "solver" => s.solver = val,
                    "fixedstep" => s.fixed_step = val,
                    "maxstep" => s.max_step = val,
                    "reltol" => s.rel_tol = val,
                    "abstol" => s.abs_tol = val,
                    other => return Err(merr!("Unknown model parameter '{}'.", other)),
                }
            } else {
                let blk = find_block(&mut model, &parts[1..])
                    .ok_or_else(|| merr!("Block '{}' not found.", path))?;
                blk.params.insert(key, val);
            }
        }
        put_model(it, parts[0], model, file)?;
        Ok(vec![])
    });
    b.insert("new_system", |it, a, _| {
        let name = a
            .first()
            .ok_or_else(|| merr!("Usage: new_system('name')"))?
            .text()?;
        if !crate::lexer::is_identifier(&name) {
            return Err(merr!("'{}' is not a valid model name.", name));
        }
        if it.open_models.contains_key(&name) {
            return Err(merr!("A model named '{}' is already open.", name));
        }
        let m = Model {
            version: super::model::FORMAT_VERSION,
            name: name.clone(),
            solver: Default::default(),
            blocks: vec![],
            lines: vec![],
        };
        it.open_models.insert(name, (m, None));
        Ok(vec![])
    });
    b.insert("load_system", |it, a, _| {
        let name = a
            .first()
            .ok_or_else(|| merr!("Usage: load_system('name')"))?
            .text()?;
        let (m, p) = load_model(it, &name)?;
        it.open_models.insert(model_key(&name), (m, Some(p)));
        Ok(vec![])
    });
    b.insert("close_system", |it, a, _| {
        match a.first() {
            Some(v) => {
                it.open_models.remove(&model_key(&v.text()?));
            }
            None => it.open_models.clear(),
        }
        Ok(vec![])
    });
    b.insert("save_system", |it, a, _| {
        let name = a
            .first()
            .ok_or_else(|| merr!("Usage: save_system('name'[, 'file.bmdl'])"))?
            .text()?;
        let key = model_key(&name);
        let (m, p) = it
            .open_models
            .get(&key)
            .cloned()
            .ok_or_else(|| merr!("Model '{}' is not open.", name))?;
        let file = match a.get(1) {
            Some(v) => {
                let t = v.text()?;
                let t = if t.to_lowercase().ends_with(".bmdl") {
                    t
                } else {
                    format!("{}.bmdl", t)
                };
                crate::builtins::resolve_path(it, &t)
            }
            None => p.unwrap_or_else(|| it.cwd.join(format!("{}.bmdl", key))),
        };
        std::fs::write(&file, m.to_json())
            .map_err(|e| merr!("Unable to save '{}': {}", file.display(), e))?;
        it.open_models.insert(key, (m, Some(file)));
        Ok(vec![])
    });
    b.insert("add_block", |it, a, _| {
        if a.len() < 2 || a.len() % 2 != 0 {
            return Err(merr!(
                "Usage: add_block('library/Type', 'model/Name', 'Param', 'value', ...)"
            ));
        }
        let src = a[0].text()?;
        let dst = a[1].text()?;
        let kind = block_type_from(&src).ok_or_else(|| merr!("Unknown block '{}'.", src))?;
        let (model_name, block_name) = dst
            .split_once('/')
            .ok_or_else(|| merr!("The destination must be 'model/BlockName'."))?;
        let (mut m, file) = get_model(it, model_name)?;
        if m.blocks.iter().any(|b| b.name == block_name) {
            return Err(merr!(
                "A block named '{}' already exists in '{}'.",
                block_name,
                model_name
            ));
        }
        let def = super::blocks::library()
            .into_iter()
            .find(|d| d.kind == kind)
            .unwrap();
        let mut params: std::collections::BTreeMap<String, String> = def
            .params
            .iter()
            .map(|p| (p.name.to_string(), p.default.to_string()))
            .collect();
        // Auto-place in a grid; 'Position', [x y w h] overrides.
        let k = m.blocks.len();
        let (mut x, mut y, mut w, mut h) = (
            60.0 + 140.0 * (k % 5) as f64,
            80.0 + 110.0 * (k / 5) as f64,
            70.0,
            50.0,
        );
        for pair in a[2..].chunks(2) {
            let key = pair[0].text()?;
            if key.eq_ignore_ascii_case("Position") {
                let p = pair[1].to_num()?.data;
                if p.len() == 4 {
                    // Accept [left top right bottom] like the reference software.
                    (x, y, w, h) = (p[0], p[1], (p[2] - p[0]).max(10.0), (p[3] - p[1]).max(10.0));
                }
                continue;
            }
            let val = match &pair[1] {
                v if v.is_text() => v.text()?,
                v => crate::display::num_to_str(v.scalar()?),
            };
            if !def.params.iter().any(|p| p.name == key) {
                return Err(merr!(
                    "Block type {} has no parameter '{}'. Parameters: {}.",
                    kind,
                    key,
                    def.params
                        .iter()
                        .map(|p| p.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            params.insert(key, val);
        }
        let id = format!(
            "b{}",
            m.blocks.len() + 1 + m.blocks.iter().filter(|b| b.id.starts_with('b')).count()
        );
        m.blocks.push(super::BlockSpec {
            id,
            kind: kind.to_string(),
            name: block_name.to_string(),
            x,
            y,
            w,
            h,
            params,
            system: None,
        });
        put_model(it, model_name, m, file)?;
        Ok(vec![])
    });
    b.insert("add_line", |it, a, _| {
        if a.len() < 3 {
            return Err(merr!(
                "Usage: add_line('model', 'Source/1', 'Destination/1')"
            ));
        }
        let model_name = a[0].text()?;
        let (mut m, file) = get_model(it, &model_name)?;
        let port = |s: &str, m: &Model| -> MResult<(String, usize)> {
            let (name, p) = s
                .rsplit_once('/')
                .ok_or_else(|| merr!("Ports are written 'BlockName/1'."))?;
            let n: usize = p
                .trim()
                .parse()
                .map_err(|_| merr!("'{}' is not a port number.", p))?;
            let b = m
                .blocks
                .iter()
                .find(|b| b.name == name)
                .ok_or_else(|| merr!("Block '{}' not found in '{}'.", name, model_name))?;
            Ok((b.id.clone(), n))
        };
        let (sb, sp) = port(&a[1].text()?, &m)?;
        let (db, dp) = port(&a[2].text()?, &m)?;
        m.lines.retain(|l| !(l.dst.block == db && l.dst.port == dp));
        m.lines.push(super::Line {
            src: super::PortRef {
                block: sb,
                port: sp,
            },
            dst: super::PortRef {
                block: db,
                port: dp,
            },
            points: vec![],
        });
        put_model(it, &model_name, m, file)?;
        Ok(vec![])
    });
    b.insert("open_system", |it, a, _| {
        let name = a.first().map(|v| v.text()).transpose()?.unwrap_or_default();
        match resolve_model(it, &name) {
            Some(p) => {
                it.print(&format!(
                    "Open {} from the Current Folder to edit it.\n",
                    p.display()
                ));
                Ok(vec![])
            }
            None => Err(merr!("Model '{}' not found.", name)),
        }
    });
}
