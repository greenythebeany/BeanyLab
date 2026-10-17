//! UI-facing API: evaluate code and report output, workspace and figure changes.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::builtins::bytes_of;
use crate::display;
use crate::error::MResult;
use crate::graphics::Figure;
use crate::interp::Interp;
use crate::value::Value;

#[derive(Debug, Clone, Serialize)]
pub struct VarInfo {
    pub name: String,
    pub size: String,
    pub class: String,
    pub value: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub output: String,
    pub error: Option<String>,
    pub workspace: Vec<VarInfo>,
    pub figures: Vec<Figure>,
    pub closed: Vec<u32>,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
}

/// Table view of a variable for the variable editor.
#[derive(Debug, Clone, Serialize)]
pub struct VarDetail {
    pub name: String,
    pub class: String,
    pub rows: usize,
    pub cols: usize,
    /// Row-major cell texts, truncated to a viewable window.
    pub cells: Vec<Vec<String>>,
    pub truncated: bool,
}

pub struct Session {
    pub interp: Interp,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            interp: Interp::new(),
        }
    }

    pub fn eval(&mut self, code: &str) -> EvalResult {
        if !code.trim().is_empty() {
            self.interp.history.push(code.to_string());
        }
        let r = self.interp.eval_string(code);
        self.finish(r)
    }

    pub fn run_file(&mut self, path: &str) -> EvalResult {
        let p = PathBuf::from(path);
        let in_cwd = p.parent().is_some_and(|d| d == self.interp.cwd);
        let label = match p.file_stem() {
            Some(stem) if in_cwd => stem.to_string_lossy().to_string(),
            _ => format!("run('{}')", path),
        };
        self.interp.history.push(label);
        let r = self.interp.run_file(&p);
        self.finish(r)
    }

    fn finish(&mut self, r: MResult<()>) -> EvalResult {
        // `clc` writes a form feed into the output stream; the UI clears the window there.
        let output = self.interp.take_output();
        let error = r.err().map(|e| e.report());
        let (figures, closed) = self.interp.graphics.take_updates();
        EvalResult {
            output,
            error,
            workspace: self.workspace(),
            figures,
            closed,
            cwd: self.cwd(),
        }
    }

    pub fn cwd(&self) -> String {
        self.interp.cwd.to_string_lossy().to_string()
    }

    pub fn set_cwd(&mut self, path: &str) -> Result<String, String> {
        let p = crate::interp::clean_path(&self.interp.cwd, path);
        if !p.is_dir() {
            return Err(format!("{} is not a folder", p.display()));
        }
        let s = p.to_string_lossy().to_string();
        self.interp.cwd = p;
        Ok(s)
    }

    pub fn workspace(&self) -> Vec<VarInfo> {
        self.interp
            .workspace()
            .into_iter()
            .map(|(name, v)| {
                let (r, c) = v.dims();
                let summary = display::summary(&v, self.interp.format);
                // "[5×5 double]" reads better as "5×5 double" in the workspace table.
                let value = if summary.starts_with('[') && summary.contains('×') {
                    summary[1..summary.len() - 1].to_string()
                } else {
                    summary
                };
                VarInfo {
                    name,
                    size: format!("{}×{}", r, c),
                    class: v.class_name().to_string(),
                    value,
                    bytes: bytes_of(&v),
                }
            })
            .collect()
    }

    pub fn var_detail(&self, name: &str) -> Option<VarDetail> {
        let v = self.interp.get_var(name)?;
        let (rows, cols) = v.dims();
        const MAX_R: usize = 500;
        const MAX_C: usize = 100;
        let fmt = self.interp.format;
        let cell_text = |r: usize, c: usize| -> String {
            match v {
                Value::Num(a, cl) => {
                    let x = *a.get(r, c);
                    if *cl == crate::value::NumClass::Logical {
                        format!("{}", x as i64)
                    } else {
                        display::fmt_scalar(x, fmt)
                    }
                }
                Value::Char(a) => a.get(r, c).to_string(),
                Value::Str(a) => format!("\"{}\"", a.get(r, c)),
                Value::Cell(a) => display::summary(a.get(r, c), fmt),
                Value::Struct(s) => format!("1×1 struct ({} fields)", s.keys.len()),
                Value::Func(f) => f.text(),
                Value::Obj(o) => o.class.to_string(),
            }
        };
        let mut cells = Vec::new();
        if let Value::Obj(o) = v {
            // Objects show their command-window text in a single cell.
            let text = crate::builtins::object_text(o, fmt);
            return Some(VarDetail {
                name: name.into(),
                class: o.class.into(),
                rows: 1,
                cols: 1,
                cells: vec![vec![text]],
                truncated: false,
            });
        }
        if let Value::Struct(s) = v {
            if s.arr.numel() == 1 {
                for (i, k) in s.keys.iter().enumerate() {
                    cells.push(vec![k.clone(), display::summary(&s.arr.data[0][i], fmt)]);
                }
                return Some(VarDetail {
                    name: name.into(),
                    class: "struct".into(),
                    rows: s.keys.len(),
                    cols: 2,
                    cells,
                    truncated: false,
                });
            }
        }
        if let Value::Char(a) = v {
            if a.rows <= 1 {
                return Some(VarDetail {
                    name: name.into(),
                    class: "char".into(),
                    rows: 1,
                    cols: 1,
                    cells: vec![vec![a.data.iter().collect()]],
                    truncated: false,
                });
            }
        }
        for r in 0..rows.min(MAX_R) {
            cells.push((0..cols.min(MAX_C)).map(|c| cell_text(r, c)).collect());
        }
        Some(VarDetail {
            name: name.into(),
            class: v.class_name().into(),
            rows,
            cols,
            cells,
            truncated: rows > MAX_R || cols > MAX_C,
        })
    }

    pub fn list_dir(&self, path: Option<&str>) -> Result<Vec<FileEntry>, String> {
        let dir = match path {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => self.interp.cwd.clone(),
        };
        list_dir(&dir)
    }

    /// Accumulated figure state (all open figures), for a UI that reconnects.
    pub fn all_figures(&self) -> Vec<Figure> {
        self.interp.graphics.figures.clone()
    }

    pub fn close_figure(&mut self, id: u32) {
        self.interp.graphics.close(Some(id));
        self.interp.graphics.closed.retain(|c| *c != id);
    }

    pub fn history(&self) -> Vec<String> {
        self.interp.history.clone()
    }

    pub fn clear_workspace(&mut self) {
        self.interp.reset_workspace();
    }

    /// Function names for completion: builtins plus `.m` files in the current folder.
    pub fn completions(&self, prefix: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .interp
            .builtins
            .keys()
            .filter(|k| k.starts_with(prefix))
            .map(|k| k.to_string())
            .collect();
        for (n, _) in self.interp.workspace() {
            if n.starts_with(prefix) {
                out.push(n);
            }
        }
        if let Ok(entries) = list_dir(&self.interp.cwd) {
            for e in entries {
                if let Some(stem) = e.name.strip_suffix(".m") {
                    if stem.starts_with(prefix) {
                        out.push(stem.to_string());
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

impl Session {
    /// Single JSON entry point shared by the desktop shell and the dev server.
    pub fn call(
        &mut self,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let s = |k: &str| args.get(k).and_then(|v| v.as_str()).map(|v| v.to_string());
        let req = |k: &str| s(k).ok_or_else(|| format!("missing argument '{}'", k));
        fn json<T: Serialize>(v: &T) -> Result<serde_json::Value, String> {
            serde_json::to_value(v).map_err(|e| e.to_string())
        }
        match method {
            "eval" => json(&self.eval(&req("code")?)),
            "run" => json(&self.run_file(&req("path")?)),
            "state" => Ok(serde_json::json!({
                "cwd": self.cwd(),
                "workspace": self.workspace(),
                "figures": self.all_figures(),
                "history": self.history(),
            })),
            "ls" => json(&self.list_dir(s("path").as_deref())?),
            "read" => {
                let p = req("path")?;
                std::fs::read_to_string(&p)
                    .map(serde_json::Value::String)
                    .map_err(|e| format!("{}: {}", p, e))
            }
            "write" => {
                let p = req("path")?;
                let content = req("content")?;
                std::fs::write(&p, content).map_err(|e| format!("{}: {}", p, e))?;
                // Edited files must be re-parsed on next call.
                self.interp.clear_cache();
                Ok(serde_json::Value::Null)
            }
            "cd" => Ok(serde_json::Value::String(self.set_cwd(&req("path")?)?)),
            "var" => json(&self.var_detail(&req("name")?)),
            "close_figure" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .ok_or("missing id")? as u32;
                self.close_figure(id);
                Ok(serde_json::Value::Null)
            }
            "complete" => json(&self.completions(&s("prefix").unwrap_or_default())),
            "clear" => {
                self.clear_workspace();
                json(&self.workspace())
            }
            "block_library" => json(&crate::sim::blocks::library()),
            "block_ports" => {
                // Port counts for editor blocks: [{type, params, system?}] -> [[in, out], ...]
                let list = args
                    .get("blocks")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let out: Vec<(usize, usize)> = list
                    .iter()
                    .map(|b| {
                        let kind = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let params: std::collections::BTreeMap<String, String> = b
                            .get("params")
                            .and_then(|p| serde_json::from_value(p.clone()).ok())
                            .unwrap_or_default();
                        let sub = b
                            .get("system")
                            .and_then(|s| {
                                serde_json::from_value::<crate::sim::Model>(s.clone()).ok()
                            })
                            .map(|m| crate::sim::subsystem_ports(&m));
                        crate::sim::blocks::ports_for(kind, &params, sub)
                    })
                    .collect();
                json(&out)
            }
            "sim" => {
                let model: crate::sim::Model =
                    serde_json::from_value(args.get("model").cloned().unwrap_or_default())
                        .map_err(|e| format!("Invalid model: {}", e))?;
                let label = s("label").unwrap_or_else(|| "model".into());
                self.interp.history.push(format!("sim('{}')", label));
                let (r, report) = match crate::sim::run::run(&mut self.interp, &model, &label) {
                    Ok((out, report)) => {
                        self.interp.set_var("out", out);
                        (self.finish(Ok(())), report)
                    }
                    Err(failure) => {
                        let (e, block) = *failure;
                        let report = crate::sim::run::RunReport {
                            displays: vec![],
                            error_block: block,
                            stop_time: 0.0,
                        };
                        (self.finish(Err(e)), report)
                    }
                };
                Ok(serde_json::json!({ "result": json(&r)?, "report": json(&report)? }))
            }
            other => Err(format!("unknown method '{}'", other)),
        }
    }
}

pub fn list_dir(dir: &Path) -> Result<Vec<FileEntry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let mut out: Vec<FileEntry> = rd
        .flatten()
        .map(|e| {
            let md = e.metadata().ok();
            FileEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
                is_dir: md.as_ref().is_some_and(|m| m.is_dir()),
                size: md.as_ref().map_or(0, |m| m.len()),
                modified: md
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            }
        })
        .filter(|e| !e.name.starts_with('.'))
        .collect();
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}
