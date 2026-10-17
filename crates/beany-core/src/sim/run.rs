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
        if log.kind != LogKind::Scope {
            continue;
        }
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
                }
                id
            }
        };
        let mut series = Vec::new();
        let mut names = Vec::new();
        let mut color = 0;
        for (p, rows) in log.data.iter().enumerate() {
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
        let show_legend = names.len() > 1;
        let ax = Axes {
            grid: (1, 1),
            cells: vec![1],
            series,
            title: Some(title.clone()),
            xlabel: Some("Time (s)".into()),
            grid_on: true,
            box_on: true,
            legend: if show_legend { Some(names) } else { None },
            legend_visible: show_legend,
            ..Default::default()
        };
        if let Some(f) = it.graphics.figures.iter_mut().find(|f| f.id == id) {
            f.axes = vec![ax];
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
    let mut displays = Vec::new();
    for log in &out.logs {
        match &log.kind {
            LogKind::ToWorkspace(var) => {
                fields.push((var.clone(), rows_to_matrix(&log.data[0], n)))
            }
            LogKind::Outport(k) => outports.push((*k, log)),
            LogKind::Display => displays.push(DisplayValue {
                id: log.id.clone(),
                value: log.data[0].last().cloned().unwrap_or_default(),
            }),
            LogKind::Scope => {}
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
        let (mut model, path) = load_model(it, &name)?;
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
        let label = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(name);
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
        let (mut model, _) = load_model(it, parts[0])?;
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
        let (mut model, file) = load_model(it, parts[0])?;
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
        std::fs::write(&file, model.to_json())
            .map_err(|e| merr!("Unable to save '{}': {}", file.display(), e))?;
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
