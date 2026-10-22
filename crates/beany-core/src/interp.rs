//! Tree-walking interpreter.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::ast::*;
use crate::display::{self, Format};
use crate::error::{MError, MResult};
use crate::graphics::{Figure, Graphics};
use crate::merr;
use crate::ops;
use crate::parser;
use crate::value::*;

pub type Builtin = fn(&mut Interp, Vec<Value>, usize) -> MResult<Vec<Value>>;

/// Random generator used by `rand`/`randn`/`randi`.
pub type Rng = StdRng;

pub fn seeded_rng(seed: u64) -> Rng {
    StdRng::seed_from_u64(seed)
}

#[derive(Default)]
pub struct Frame {
    pub vars: HashMap<String, Value>,
    pub globals: HashSet<String>,
    pub persist: HashSet<String>,
    pub persist_key: String,
    pub unit: Option<Arc<FileUnit>>,
    pub name: String,
    pub nargin: usize,
    pub nargout: usize,
    pub line: usize,
}

#[derive(Debug, PartialEq)]
pub enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

pub struct OpenFile {
    pub path: PathBuf,
    pub write: bool,
    pub buf: Vec<u8>,
    pub pos: usize,
}

pub struct Interp {
    pub frames: Vec<Frame>,
    pub globals: HashMap<String, Value>,
    pub persistents: HashMap<String, HashMap<String, Value>>,
    pub builtins: HashMap<&'static str, Builtin>,
    /// Versions of builtins used when an argument is complex.
    pub complex_builtins: HashMap<&'static str, Builtin>,
    /// Versions of builtins used when an argument is a linear system model.
    pub lti_builtins: HashMap<&'static str, Builtin>,
    pub cwd: PathBuf,
    pub path: Vec<PathBuf>,
    cache: HashMap<PathBuf, (Option<SystemTime>, Arc<FileUnit>)>,
    pub graphics: Graphics,
    out: String,
    pub stream: Option<Box<dyn FnMut(&str) + Send>>,
    pub figure_stream: Option<Box<dyn FnMut(Vec<Figure>, Vec<u32>) + Send>>,
    pub interrupt: Arc<AtomicBool>,
    /// Set by the UI to hold a running simulation; cleared to resume.
    pub pause: Arc<AtomicBool>,
    /// Named UI events such as simulation progress (desktop only).
    pub event_stream: Option<Box<dyn FnMut(&str, serde_json::Value) + Send>>,
    pub format: Format,
    pub rng: StdRng,
    end_ctx: Vec<(usize, usize, usize, usize)>,
    pub tic: Instant,
    depth: usize,
    pub warnings_on: bool,
    pub last_warning: String,
    pub files: HashMap<i32, OpenFile>,
    pub history: Vec<String>,
    /// Name of the builtin currently being dispatched (lets one body serve several names).
    pub last_builtin: String,
    /// Variable names of plain-identifier arguments, for functions like `table`
    /// that name their outputs after their inputs.
    pub arg_names: Vec<Option<String>>,
    /// Block-diagram models loaded or built by scripts (new_system/load_system), by name.
    pub open_models: HashMap<String, (crate::sim::Model, Option<PathBuf>)>,
    /// setappdata/getappdata values, keyed by handle and name.
    pub appdata: HashMap<String, Value>,
    /// Loaded user classes by name.
    pub classes: HashMap<String, Arc<crate::classes::ClassInfo>>,
    /// Function names found in `private/` folders, by folder.
    private_fns: HashMap<PathBuf, HashSet<String>>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        let mut it = Interp {
            frames: vec![Frame {
                name: "base".into(),
                ..Default::default()
            }],
            globals: HashMap::new(),
            persistents: HashMap::new(),
            builtins: HashMap::new(),
            complex_builtins: HashMap::new(),
            lti_builtins: HashMap::new(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            path: Vec::new(),
            cache: HashMap::new(),
            graphics: Graphics::default(),
            out: String::new(),
            stream: None,
            figure_stream: None,
            interrupt: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            event_stream: None,
            format: Format::Short,
            rng: StdRng::seed_from_u64(0x5eed),
            end_ctx: Vec::new(),
            tic: Instant::now(),
            depth: 0,
            warnings_on: true,
            last_warning: String::new(),
            files: HashMap::new(),
            history: Vec::new(),
            last_builtin: String::new(),
            arg_names: Vec::new(),
            open_models: HashMap::new(),
            classes: HashMap::new(),
            appdata: HashMap::new(),
            private_fns: HashMap::new(),
        };
        crate::builtins::register(&mut it);
        crate::builtins::addons::init(&mut it);
        it
    }

    // ---------- output ----------

    pub fn print(&mut self, s: &str) {
        if let Some(cb) = self.stream.as_mut() {
            cb(s);
        } else {
            self.out.push_str(s);
        }
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.out)
    }

    /// Push pending figure changes to the UI immediately (drawnow / pause).
    pub fn flush_graphics(&mut self) {
        if self.figure_stream.is_some() {
            let (figs, closed) = self.graphics.take_updates();
            if !figs.is_empty() || !closed.is_empty() {
                if let Some(cb) = self.figure_stream.as_mut() {
                    cb(figs, closed);
                }
            }
        }
    }

    /// Send a named event to the UI, if one is listening.
    pub fn emit(&mut self, name: &str, payload: serde_json::Value) {
        if let Some(cb) = self.event_stream.as_mut() {
            cb(name, payload);
        }
    }

    pub fn warn(&mut self, msg: &str) {
        self.last_warning = msg.to_string();
        if self.warnings_on {
            self.print(&format!("Warning: {}\n", msg));
        }
    }

    fn check_interrupt(&self) -> MResult<()> {
        if self.interrupt.load(Ordering::Relaxed) {
            self.interrupt.store(false, Ordering::Relaxed);
            return Err(MError::interrupted());
        }
        Ok(())
    }

    // ---------- variables ----------

    pub fn frame(&self) -> &Frame {
        self.frames.last().unwrap()
    }
    pub fn frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    pub fn get_var(&self, name: &str) -> Option<&Value> {
        let f = self.frame();
        if f.globals.contains(name) {
            self.globals.get(name)
        } else if f.persist.contains(name) {
            self.persistents
                .get(&f.persist_key)
                .and_then(|m| m.get(name))
        } else {
            f.vars.get(name)
        }
    }

    pub fn is_var(&self, name: &str) -> bool {
        self.get_var(name).is_some()
    }

    pub fn set_var(&mut self, name: &str, v: Value) {
        let f = self.frames.last_mut().unwrap();
        if f.globals.contains(name) {
            self.globals.insert(name.to_string(), v);
        } else if f.persist.contains(name) {
            let key = f.persist_key.clone();
            self.persistents
                .entry(key)
                .or_default()
                .insert(name.to_string(), v);
        } else {
            f.vars.insert(name.to_string(), v);
        }
    }

    pub fn clear_var(&mut self, name: &str) {
        let f = self.frames.last_mut().unwrap();
        f.vars.remove(name);
        f.globals.remove(name);
    }

    /// Sorted workspace listing of the current frame.
    pub fn workspace(&self) -> Vec<(String, Value)> {
        let f = self.frame();
        let mut names: Vec<&String> = f.vars.keys().collect();
        names.extend(f.globals.iter());
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
            .into_iter()
            .filter_map(|n| self.get_var(n).map(|v| (n.clone(), v.clone())))
            .collect()
    }

    // ---------- entry points ----------

    /// Evaluate text typed at the command window (or `eval`).
    pub fn eval_string(&mut self, code: &str) -> MResult<()> {
        let unit = Arc::new(parser::parse_snippet(code)?);
        if unit.script.is_none() {
            return Err(merr!("Function definitions are not supported in this context. Functions can only be created as local or nested functions in code files."));
        }
        self.run_script_unit(&unit)
    }

    fn run_script_unit(&mut self, unit: &Arc<FileUnit>) -> MResult<()> {
        let saved = self.frame_mut().unit.replace(unit.clone());
        let saved_line = self.frame().line;
        let r = self.exec_block(unit.script.as_deref().unwrap_or(&[]));
        let line = self.frame().line;
        self.frame_mut().unit = saved;
        self.frame_mut().line = saved_line;
        match r {
            Ok(_) => Ok(()),
            Err(mut e) => {
                if !unit.path.is_empty() && !e.interrupt {
                    let name = Path::new(&unit.path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    e.stack.push(format!("{} (line {})", name, line));
                }
                Err(e)
            }
        }
    }

    /// Run a script file (or call a function file without arguments).
    pub fn run_file(&mut self, path: &Path) -> MResult<()> {
        let unit = self.load_unit(path)?;
        if unit.script.is_some() {
            if let Some(dir) = path.parent() {
                // Scripts run with their folder temporarily on the path so helpers resolve.
                let dir = dir.to_path_buf();
                let added = !self.path.contains(&dir) && dir != self.cwd;
                if added {
                    self.path.insert(0, dir.clone());
                }
                let r = self.run_script_unit(&unit);
                if added {
                    self.path.retain(|p| *p != dir);
                }
                return r;
            }
            return self.run_script_unit(&unit);
        }
        let f = unit.functions[0].clone();
        let vals = self.call_user(&unit, &f, Vec::new(), 0)?;
        if let Some(v) = vals.into_iter().next() {
            self.set_var("ans", v.clone());
            let s = display::display("ans", &v, self.format);
            self.print(&s);
        }
        Ok(())
    }

    pub fn load_unit(&mut self, path: &Path) -> MResult<Arc<FileUnit>> {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((t, u)) = self.cache.get(path) {
            if *t == mtime && mtime.is_some() {
                return Ok(u.clone());
            }
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| merr!("Unable to read file '{}': {}", path.display(), e))?;
        let unit = Arc::new(parser::parse_file(&path.to_string_lossy(), &src)?);
        self.cache.insert(path.to_path_buf(), (mtime, unit.clone()));
        Ok(unit)
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.classes.clear();
        self.private_fns.clear();
    }

    /// Locate `name.m` in the current folder, then the path.
    pub fn find_file(&self, name: &str, include_path: bool) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) {
            return None;
        }
        // `pkg.sub.fn` lives in `+pkg/+sub/fn.m`.
        let file = match name.rsplit_once('.') {
            Some((pkg, f)) => {
                if f.is_empty() || pkg.split('.').any(|p| p.is_empty()) {
                    return None;
                }
                let mut p = PathBuf::new();
                for part in pkg.split('.') {
                    p.push(format!("+{}", part));
                }
                p.push(format!("{}.m", f));
                p
            }
            None => PathBuf::from(format!("{}.m", name)),
        };
        let p = self.cwd.join(&file);
        if p.is_file() {
            return Some(p);
        }
        if include_path {
            for dir in &self.path {
                let p = dir.join(&file);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }

    // ---------- statements ----------

    pub fn exec_block(&mut self, stmts: &[Stmt]) -> MResult<Flow> {
        for s in stmts {
            let flow = self.exec(s)?;
            if flow != Flow::Normal {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn show(&mut self, name: &str, v: &Value) {
        if let Value::Inst(o) = v {
            if let Some(m) = o.class.methods.get("display").cloned() {
                if self.call_method(&m, vec![v.clone()], 0).is_ok() {
                    return;
                }
            } else if let Some(m) = o.class.methods.get("disp").cloned() {
                self.print(&format!("{} = \n\n", name));
                if self.call_method(&m, vec![v.clone()], 0).is_ok() {
                    self.print("\n");
                    return;
                }
            }
        }
        let s = display::display(name, v, self.format);
        self.print(&s);
    }

    fn exec(&mut self, s: &Stmt) -> MResult<Flow> {
        self.check_interrupt()?;
        self.frame_mut().line = s.line;
        match &s.kind {
            StmtKind::Expr { expr, print } => {
                if let Expr::Ident(name) = expr {
                    if let Some(v) = self.get_var(name).cloned() {
                        if *print {
                            self.show(name, &v);
                        }
                        return Ok(Flow::Normal);
                    }
                }
                let vals = self.eval_multi(expr, 0)?;
                let is_call = is_call_expr(expr, self);
                for (i, v) in vals.into_iter().enumerate() {
                    if is_call && i > 0 {
                        break;
                    }
                    self.set_var("ans", v.clone());
                    if *print {
                        self.show("ans", &v);
                    }
                }
            }
            StmtKind::Assign { lhs, rhs, print } => {
                if lhs.len() == 1 {
                    let v = self.eval(rhs)?;
                    self.assign(&lhs[0], v)?;
                } else {
                    let vals = self.eval_multi(rhs, lhs.len())?;
                    let needed = lhs
                        .iter()
                        .rposition(|l| !matches!(l, LValue::Ignore))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    if vals.len() < needed {
                        return Err(merr!("Insufficient number of outputs from right hand side of equal sign to satisfy assignment."));
                    }
                    for (l, v) in lhs.iter().zip(vals) {
                        self.assign(l, v)?;
                    }
                }
                if *print {
                    for l in lhs {
                        if let LValue::Var { name, .. } = l {
                            if let Some(v) = self.get_var(name).cloned() {
                                self.show(name, &v);
                            }
                        }
                    }
                }
            }
            StmtKind::If { clauses, else_body } => {
                for (cond, body) in clauses {
                    if self.eval(cond)?.is_true()? {
                        return self.exec_block(body);
                    }
                }
                if let Some(b) = else_body {
                    return self.exec_block(b);
                }
            }
            StmtKind::For { var, iter, body } => return self.exec_for(var, iter, body),
            StmtKind::While { cond, body } => loop {
                self.check_interrupt()?;
                if !self.eval(cond)?.is_true()? {
                    break;
                }
                match self.exec_block(body)? {
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                    _ => {}
                }
            },
            StmtKind::Switch {
                subject,
                cases,
                otherwise,
            } => {
                let subj = self.eval(subject)?;
                for (cv, body) in cases {
                    let c = self.eval(cv)?;
                    if switch_match(&subj, &c) {
                        return self.exec_block(body);
                    }
                }
                if let Some(b) = otherwise {
                    return self.exec_block(b);
                }
            }
            StmtKind::Try {
                body,
                ident,
                catch_body,
            } => {
                let depth = self.frames.len();
                match self.exec_block(body) {
                    Ok(f) => return Ok(f),
                    Err(e) if e.interrupt => return Err(e),
                    Err(e) => {
                        self.frames.truncate(depth);
                        self.end_ctx.clear();
                        if let Some(id) = ident {
                            self.set_var(id, exception_value(&e));
                        }
                        return self.exec_block(catch_body);
                    }
                }
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Return => return Ok(Flow::Return),
            StmtKind::Global(names) => {
                for n in names {
                    let local = self.frame_mut().vars.remove(n);
                    self.globals
                        .entry(n.clone())
                        .or_insert_with(|| local.unwrap_or_else(Value::empty));
                    self.frame_mut().globals.insert(n.clone());
                }
            }
            StmtKind::Persistent(names) => {
                let key = self.frame().persist_key.clone();
                for n in names {
                    self.persistents
                        .entry(key.clone())
                        .or_default()
                        .entry(n.clone())
                        .or_insert_with(Value::empty);
                    self.frame_mut().persist.insert(n.clone());
                }
            }
            StmtKind::Command {
                name,
                args,
                raw,
                print,
            } => {
                if self.is_var(name) {
                    let unit = parser::parse_snippet(raw)?;
                    let mut body = unit.script.unwrap_or_default();
                    if !*print {
                        for st in body.iter_mut() {
                            if let StmtKind::Expr { print, .. } | StmtKind::Assign { print, .. } =
                                &mut st.kind
                            {
                                *print = false;
                            }
                        }
                    }
                    for st in body.iter_mut() {
                        st.line = s.line;
                    }
                    return self.exec_block(&body);
                }
                let argv: Vec<Value> = args.iter().map(|a| Value::chars(a)).collect();
                let vals = self.call_named(name, argv, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    self.set_var("ans", v.clone());
                    if *print {
                        self.show("ans", &v);
                    }
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_for(&mut self, var: &str, iter: &Expr, body: &[Stmt]) -> MResult<Flow> {
        // Fast path for numeric ranges: no array allocation.
        if let Expr::Range(a, step, b) = iter {
            let start = self.eval(a)?.scalar()?;
            let step = match step {
                Some(s) => self.eval(s)?.scalar()?,
                None => 1.0,
            };
            let stop = self.eval(b)?.scalar()?;
            let n = if step == 0.0 || (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
                0
            } else {
                ((stop - start) / step + 1e-10).floor() as usize + 1
            };
            if n == 0 {
                self.set_var(var, Value::Num(Array::new(1, 0, vec![]), NumClass::Double));
            }
            for i in 0..n {
                self.check_interrupt()?;
                self.set_var(var, Value::num(start + step * i as f64));
                match self.exec_block(body)? {
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                    _ => {}
                }
            }
            return Ok(Flow::Normal);
        }
        let v = self.eval(iter)?;
        let (_, cols) = v.dims();
        if v.is_empty() {
            self.set_var(var, Value::empty());
            return Ok(Flow::Normal);
        }
        for c in 0..cols {
            self.check_interrupt()?;
            let item = v.column(c)?;
            self.set_var(var, item);
            match self.exec_block(body)? {
                Flow::Break => break,
                Flow::Return => return Ok(Flow::Return),
                _ => {}
            }
        }
        Ok(Flow::Normal)
    }

    // ---------- assignment ----------

    pub fn assign(&mut self, lv: &LValue, rhs: Value) -> MResult<()> {
        match lv {
            LValue::Ignore => Ok(()),
            LValue::Var { name, path } => {
                if path.is_empty() {
                    self.set_var(name, rhs);
                    return Ok(());
                }
                let cur = self.get_var(name).cloned();
                let new = self.assign_path(cur, path, rhs)?;
                self.set_var(name, new);
                Ok(())
            }
        }
    }

    fn assign_path(&mut self, cur: Option<Value>, path: &[Access], rhs: Value) -> MResult<Value> {
        // An empty [] behaves like an undefined variable for structural assignment.
        let cur = match cur {
            Some(Value::Num(a, NumClass::Double))
                if a.rows == 0 && a.cols == 0 && !matches!(path[0], Access::Paren(_)) =>
            {
                None
            }
            c => c,
        };
        match &path[0] {
            Access::Field(_) | Access::DynField(_) => {
                let field = match &path[0] {
                    Access::Field(f) => f.clone(),
                    Access::DynField(e) => self.eval(e)?.text()?,
                    _ => unreachable!(),
                };
                if let Some(Value::Inst(o)) = cur {
                    let new = if path.len() == 1 {
                        rhs
                    } else {
                        let child = self
                            .get_prop(&o, &field)?
                            .into_iter()
                            .next()
                            .unwrap_or_else(Value::empty);
                        self.assign_path(Some(child), &path[1..], rhs)?
                    };
                    return self.set_prop(o, &field, new);
                }
                if let Some(Value::Obj(mut o)) = cur {
                    let new = if path.len() == 1 {
                        rhs
                    } else {
                        let child = crate::builtins::objects::get_field(&o, &field)?;
                        self.assign_path(Some(child), &path[1..], rhs)?
                    };
                    crate::builtins::objects::set_field(&mut o, &field, new)?;
                    return Ok(Value::Obj(o));
                }
                let mut s = match cur {
                    None => StructArr::scalar(vec![]),
                    Some(Value::Struct(s)) => s,
                    Some(_) => return Err(merr!("Unable to perform assignment because dot indexing is not supported for variables of this type.")),
                };
                if s.arr.numel() != 1 {
                    if path.len() == 1 && s.arr.numel() == 0 {
                        s.arr = Array::scalar(s.empty_elem());
                    } else {
                        return Err(merr!("Scalar structure required for this assignment."));
                    }
                }
                let k = s.add_key(&field);
                let new = if path.len() == 1 {
                    rhs
                } else {
                    let child = s.arr.data[0][k].clone();
                    self.assign_path(Some(child), &path[1..], rhs)?
                };
                s.arr.data[0][k] = new;
                Ok(Value::Struct(s))
            }
            Access::Paren(args) => {
                if path.len() == 1 {
                    let base = cur.unwrap_or_else(|| empty_like(&rhs));
                    let idx = self.eval_index_args(&base, args)?;
                    return paren_assign(base, &idx, rhs);
                }
                // x(i).field = v  /  x(i){...} = v
                let base = match cur {
                    None => Value::Struct(StructArr { keys: vec![], arr: Array::empty() }),
                    Some(v @ Value::Struct(_)) => v,
                    Some(v @ Value::Cell(_)) => v,
                    Some(Value::Num(a, _)) if a.is_empty() => Value::Struct(StructArr { keys: vec![], arr: Array::empty() }),
                    Some(_) => return Err(merr!("Unable to perform assignment because this type of variable does not support field or brace indexing.")),
                };
                let idx = self.eval_index_args(&base, args)?;
                match base {
                    Value::Struct(mut s) => {
                        let child = match s.arr.index(&idx) {
                            Ok(a) if a.numel() == 1 => Some(Value::Struct(StructArr { keys: s.keys.clone(), arr: a })),
                            Ok(a) if a.numel() > 1 => return Err(merr!("Expected one output from a curly brace or dot indexing expression, but there were {} results.", a.numel())),
                            _ => None,
                        };
                        let new = self.assign_path(
                            child.or_else(|| {
                                Some(Value::Struct(StructArr {
                                    keys: s.keys.clone(),
                                    arr: Array::scalar(s.empty_elem()),
                                }))
                            }),
                            &path[1..],
                            rhs,
                        )?;
                        let Value::Struct(ns) = new else {
                            return Err(merr!("Invalid struct array assignment"));
                        };
                        for k in &ns.keys {
                            s.add_key(k);
                        }
                        let elem: Vec<Value> = s
                            .keys
                            .iter()
                            .map(|k| ns.get(0, k).cloned().unwrap_or_else(Value::empty))
                            .collect();
                        let fill = s.empty_elem();
                        s.arr.assign(&idx, &Array::scalar(elem), &fill)?;
                        Ok(Value::Struct(s))
                    }
                    Value::Cell(mut c) => {
                        let child = match c.index(&idx) {
                            Ok(a) if a.numel() == 1 => Some(Value::Cell(a)),
                            _ => None,
                        };
                        let new = self.assign_path(child, &path[1..], rhs)?;
                        let Value::Cell(nc) = new else {
                            return Err(merr!("Invalid cell assignment"));
                        };
                        c.assign(&idx, &nc, &Value::empty())?;
                        Ok(Value::Cell(c))
                    }
                    _ => unreachable!(),
                }
            }
            Access::Brace(args) => {
                if let Some(Value::Obj(mut o)) = cur {
                    if path.len() > 1 {
                        return Err(merr!(
                            "Nested assignment inside {} braces is not supported.",
                            o.class
                        ));
                    }
                    let base = Value::Obj(o.clone());
                    let idx = self.eval_index_args(&base, args)?;
                    crate::builtins::objects::brace_set(&mut o, &idx, rhs)?;
                    return Ok(Value::Obj(o));
                }
                let mut c = match cur {
                    None => Array::empty(),
                    Some(Value::Cell(c)) => c,
                    Some(Value::Num(a, _)) if a.is_empty() => Array::empty(),
                    Some(_) => return Err(merr!("Unable to perform assignment because brace indexing is not supported for variables of this type.")),
                };
                let cv = Value::Cell(c.clone());
                let idx = self.eval_index_args(&cv, args)?;
                let new = if path.len() == 1 {
                    rhs
                } else {
                    let child = match c.index(&idx) {
                        Ok(a) if a.numel() == 1 => Some(a.data[0].clone()),
                        _ => None,
                    };
                    self.assign_path(child, &path[1..], rhs)?
                };
                c.assign(&idx, &Array::scalar(new), &Value::empty())?;
                Ok(Value::Cell(c))
            }
        }
    }

    // ---------- expressions ----------

    pub fn eval(&mut self, e: &Expr) -> MResult<Value> {
        match e {
            Expr::Num(n) => Ok(Value::num(*n)),
            Expr::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(v.clone());
                }
                let mut vals = self.call_named(name, vec![], 1)?;
                if vals.is_empty() {
                    return Err(merr!("Too many output arguments."));
                }
                Ok(vals.swap_remove(0))
            }
            _ => {
                let mut vals = self.eval_multi(e, 1)?;
                if vals.is_empty() {
                    return Err(match e {
                        Expr::CellIndex(..) | Expr::Field(..) => merr!("Expected one output from a curly brace or dot indexing expression, but there were 0 results."),
                        _ => merr!("Too many output arguments."),
                    });
                }
                Ok(vals.swap_remove(0))
            }
        }
    }

    pub fn eval_multi(&mut self, e: &Expr, nargout: usize) -> MResult<Vec<Value>> {
        Ok(match e {
            Expr::Num(n) => vec![Value::num(*n)],
            Expr::Imag(n) => vec![Value::Complex(Array::scalar(C64::new(0.0, *n)))],
            Expr::Char(s) => vec![Value::chars(s)],
            Expr::Str(s) => vec![Value::string(s)],
            Expr::Colon => vec![Value::chars(":")],
            Expr::End => {
                let Some(&(rows, cols, k, n)) = self.end_ctx.last() else {
                    return Err(merr!("'end' is only valid inside an index expression."));
                };
                let v = if n == 1 {
                    rows * cols
                } else if k == 0 {
                    rows
                } else if k == 1 {
                    cols
                } else {
                    1
                };
                vec![Value::num(v as f64)]
            }
            Expr::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(vec![v.clone()]);
                }
                self.call_named(name, vec![], nargout)?
            }
            Expr::Paren(inner) => vec![self.eval(inner)?],
            Expr::Unary(op, a) => {
                let v = self.eval(a)?;
                if let Value::Inst(_) = v {
                    let (m, sym) = match op {
                        UnOp::Neg => ("uminus", "-"),
                        UnOp::Plus => ("uplus", "+"),
                        UnOp::Not => ("not", "~"),
                    };
                    if *op == UnOp::Plus
                        && self.dispatch_method(m, std::slice::from_ref(&v)).is_none()
                    {
                        return Ok(vec![v]);
                    }
                    return Ok(vec![self.object_operator(m, sym, vec![v])?]);
                }
                if crate::builtins::control::is_lti(&v) {
                    return Ok(vec![crate::builtins::control::lti_unary(*op, &v)?]);
                }
                vec![ops::unary(*op, &v)?]
            }
            Expr::Binary(BinOp::AndAnd, a, b) => {
                let l = self.eval(a)?;
                let lt = logical_scalar(&l, "&&")?;
                if !lt {
                    return Ok(vec![Value::boolean(false)]);
                }
                let r = self.eval(b)?;
                vec![Value::boolean(logical_scalar(&r, "&&")?)]
            }
            Expr::Binary(BinOp::OrOr, a, b) => {
                let l = self.eval(a)?;
                if logical_scalar(&l, "||")? {
                    return Ok(vec![Value::boolean(true)]);
                }
                let r = self.eval(b)?;
                vec![Value::boolean(logical_scalar(&r, "||")?)]
            }
            Expr::Binary(op, a, b) => {
                let l = self.eval(a)?;
                let r = self.eval(b)?;
                if matches!(l, Value::Inst(_)) || matches!(r, Value::Inst(_)) {
                    let m = crate::classes::binop_method(*op);
                    return Ok(vec![self.object_operator(m, m, vec![l, r])?]);
                }
                if crate::builtins::control::is_lti(&l) || crate::builtins::control::is_lti(&r) {
                    return Ok(vec![crate::builtins::control::lti_binary(*op, &l, &r)?]);
                }
                vec![ops::binary(*op, &l, &r)?]
            }
            Expr::Transpose(a) => {
                let v = self.eval(a)?;
                if let Value::Inst(_) = v {
                    for m in ["ctranspose", "transpose"] {
                        if self.dispatch_method(m, std::slice::from_ref(&v)).is_some() {
                            return Ok(vec![self.object_operator(m, "'", vec![v])?]);
                        }
                    }
                }
                if let Value::Complex(a) = &v {
                    return Ok(vec![Value::Complex(a.transpose().map(|z| z.conj()))]);
                }
                vec![v.transpose()?]
            }
            Expr::DotTranspose(a) => {
                let v = self.eval(a)?;
                if let Value::Inst(_) = v {
                    return Ok(vec![self.object_operator("transpose", ".'", vec![v])?]);
                }
                vec![v.transpose()?]
            }
            Expr::SuperCall(name, sup, args) => {
                let argv = self.eval_args(args)?;
                self.super_call(name, sup, argv, nargout)?
            }
            Expr::Range(a, step, b) => {
                let start = self.eval(a)?.scalar()?;
                let st = match step {
                    Some(s) => self.eval(s)?.scalar()?,
                    None => 1.0,
                };
                let stop = self.eval(b)?.scalar()?;
                vec![Value::mat(ops::range(start, st, stop)?)]
            }
            Expr::Matrix(rows) => {
                let mut row_vals = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut items = Vec::with_capacity(row.len());
                    for item in row {
                        items.extend(self.eval_multi(item, 1)?);
                    }
                    if items.iter().any(|v| matches!(v, Value::Inst(_))) {
                        row_vals.push(self.concat_objects(items, true)?);
                    } else {
                        row_vals.push(ops::horzcat(items)?);
                    }
                }
                if row_vals.is_empty() {
                    vec![Value::empty()]
                } else if row_vals.iter().any(|v| matches!(v, Value::Inst(_))) {
                    vec![self.concat_objects(row_vals, false)?]
                } else {
                    vec![ops::vertcat(row_vals)?]
                }
            }
            Expr::Cell(rows) => {
                let mut row_vals = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut items = Vec::with_capacity(row.len());
                    for item in row {
                        for v in self.eval_multi(item, 1)? {
                            items.push(match v {
                                Value::Cell(c) => Value::Cell(c),
                                other => Value::Cell(Array::scalar(other)),
                            });
                        }
                    }
                    if !items.is_empty() {
                        row_vals.push(ops::horzcat(items)?);
                    }
                }
                if row_vals.is_empty() {
                    vec![Value::Cell(Array::empty())]
                } else {
                    vec![ops::vertcat(row_vals)?]
                }
            }
            Expr::AnonFn { params, body, text } => {
                let mut names = HashSet::new();
                collect_idents(body, &mut names);
                let mut captured = HashMap::new();
                for n in names {
                    if params.contains(&n) {
                        continue;
                    }
                    if let Some(v) = self.get_var(&n) {
                        captured.insert(n, v.clone());
                    }
                }
                vec![Value::Func(FuncHandle::Anon {
                    params: params.clone(),
                    body: body.clone(),
                    captured: Arc::new(captured),
                    text: text.clone(),
                    unit: self.frame().unit.clone(),
                })]
            }
            Expr::FuncHandle(name) => vec![Value::Func(FuncHandle::Named(name.clone()))],
            Expr::Index(base, args) => {
                if let Some(name) = self.dotted_function(base) {
                    let argv = self.eval_args(args)?;
                    return self.call_named(&name, argv, nargout);
                }
                if let Expr::Field(ob, member) = base.as_ref() {
                    if let Some(r) = self.member_call(ob, member, Some(args), nargout)? {
                        return Ok(r);
                    }
                }
                if let Expr::Ident(name) = base.as_ref() {
                    if !self.is_var(name) {
                        let argv = self.eval_args(args)?;
                        if name == "table" {
                            self.arg_names = args
                                .iter()
                                .map(|a| match a {
                                    Expr::Ident(n) if self.is_var(n) => Some(n.clone()),
                                    _ => None,
                                })
                                .collect();
                        }
                        return self.call_named(name, argv, nargout);
                    }
                }
                let v = self.eval(base)?;
                if let Value::Func(fh) = &v {
                    let argv = self.eval_args(args)?;
                    return self.call_handle(fh, argv, nargout);
                }
                let idx = self.eval_index_args(&v, args)?;
                vec![v.index(&idx)?]
            }
            Expr::CellIndex(base, args) => {
                let v = self.eval(base)?;
                if let Value::Obj(o) = &v {
                    let idx = self.eval_index_args(&v, args)?;
                    return crate::builtins::objects::brace_get(o, &idx);
                }
                let Value::Cell(c) = &v else {
                    return Err(merr!(
                        "Brace indexing is not supported for variables of this type."
                    ));
                };
                let idx = self.eval_index_args(&v, args)?;
                c.index(&idx)?.data
            }
            Expr::Field(base, name) => {
                if let Some(f) = self.dotted_function(e) {
                    return self.call_named(&f, vec![], nargout);
                }
                if let Some(r) = self.member_call(base, name, None, nargout)? {
                    return Ok(r);
                }
                let v = self.eval(base)?;
                field_values(&v, name)?
            }
            Expr::DynField(base, f) => {
                let v = self.eval(base)?;
                let name = self.eval(f)?.text()?;
                field_values(&v, &name)?
            }
        })
    }

    /// Arguments for a function call: cs-lists expand, `:` becomes the char ':'.
    pub fn eval_args(&mut self, args: &[Expr]) -> MResult<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Expr::Colon => out.push(Value::chars(":")),
                Expr::CellIndex(..) | Expr::Field(..) | Expr::DynField(..) => {
                    out.extend(self.eval_multi(a, 1)?)
                }
                _ => out.push(self.eval(a)?),
            }
        }
        Ok(out)
    }

    fn eval_index_args(&mut self, base: &Value, args: &[Expr]) -> MResult<Vec<Idx>> {
        let (rows, cols) = base.dims();
        let n = args.len();
        let mut out = Vec::with_capacity(n);
        for (k, a) in args.iter().enumerate() {
            if let Expr::Colon = a {
                out.push(Idx::All);
                continue;
            }
            self.end_ctx.push((rows, cols, k, n));
            let r = self.eval(a);
            self.end_ctx.pop();
            let v = r?;
            if let Value::Char(c) = &v {
                if c.numel() == 1 && c.data[0] == ':' {
                    out.push(Idx::All);
                    continue;
                }
            }
            let len = if n == 1 {
                rows * cols
            } else if k == 0 {
                rows
            } else if k == 1 {
                cols
            } else {
                1
            };
            out.push(to_idx(&v, len)?);
        }
        Ok(out)
    }

    // ---------- calls ----------

    /// `obj.member(args)` / `Class.member(args)` when the member is a method, a
    /// constant or an object property. `None` leaves the expression to plain field access.
    fn member_call(
        &mut self,
        ob: &Expr,
        member: &str,
        args: Option<&[Expr]>,
        nargout: usize,
    ) -> MResult<Option<Vec<Value>>> {
        if let Expr::Ident(cn) = ob {
            if !self.is_var(cn)
                && self
                    .frame()
                    .unit
                    .as_ref()
                    .is_none_or(|u| u.find(cn).is_none())
            {
                match self.class_member(cn, member)? {
                    Some(crate::classes::ClassMember::Method(m)) => {
                        let argv = match args {
                            Some(a) => self.eval_args(a)?,
                            None => Vec::new(),
                        };
                        return self.call_method(&m, argv, nargout).map(Some);
                    }
                    Some(crate::classes::ClassMember::Value(v)) => {
                        return match args {
                            Some(a) => {
                                let idx = self.eval_index_args(&v, a)?;
                                Ok(Some(vec![v.index(&idx)?]))
                            }
                            None => Ok(Some(vec![v])),
                        };
                    }
                    None => {}
                }
            }
        }
        // Only objects are handled here; evaluate the base once.
        if !self.may_be_object(ob) {
            return Ok(None);
        }
        let ov = self.eval(ob)?;
        let Value::Inst(o) = &ov else {
            let vals = field_values(&ov, member)?;
            return match args {
                None => Ok(Some(vals)),
                Some(a) => {
                    let v = vals
                        .into_iter()
                        .next()
                        .ok_or_else(|| merr!("Field is empty."))?;
                    if let Value::Func(fh) = &v {
                        let argv = self.eval_args(a)?;
                        return self.call_handle(fh, argv, nargout).map(Some);
                    }
                    let idx = self.eval_index_args(&v, a)?;
                    Ok(Some(vec![v.index(&idx)?]))
                }
            };
        };
        if !o.class.has_prop(member) {
            if let Some(m) = o.class.methods.get(member).cloned() {
                let mut argv = vec![ov.clone()];
                if let Some(a) = args {
                    argv.extend(self.eval_args(a)?);
                }
                return self.call_method(&m, argv, nargout).map(Some);
            }
        }
        let vals = self.get_prop(o, member)?;
        match args {
            None => Ok(Some(vals)),
            Some(a) => {
                let v = vals.into_iter().next().unwrap_or_else(Value::empty);
                if let Value::Func(fh) = &v {
                    let argv = self.eval_args(a)?;
                    return self.call_handle(fh, argv, nargout).map(Some);
                }
                let idx = self.eval_index_args(&v, a)?;
                Ok(Some(vec![v.index(&idx)?]))
            }
        }
    }

    /// The expression can evaluate to an object: a variable holding one, or anything
    /// that is not a plain variable (calls, nested access).
    fn may_be_object(&self, e: &Expr) -> bool {
        match e {
            Expr::Ident(n) => matches!(self.get_var(n), Some(Value::Inst(_))),
            Expr::Field(b, _) | Expr::Index(b, _) => self.may_be_object(b),
            _ => false,
        }
    }

    /// `a.b.c` names a package function (or a dotted built-in) when `a` is not a variable.
    fn dotted_function(&self, e: &Expr) -> Option<String> {
        fn join(e: &Expr) -> Option<(String, String)> {
            match e {
                Expr::Ident(n) => Some((n.clone(), n.clone())),
                Expr::Field(b, n) => join(b).map(|(root, s)| (root, format!("{}.{}", s, n))),
                _ => None,
            }
        }
        let Expr::Field(..) = e else {
            return None;
        };
        let (root, name) = join(e)?;
        if self.is_var(&root) {
            return None;
        }
        (self.builtins.contains_key(name.as_str()) || self.find_file(&name, true).is_some())
            .then_some(name)
    }

    pub fn call_named(
        &mut self,
        name: &str,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        if let Some(unit) = self.frame().unit.clone() {
            // In a class-folder method file the main function is the method itself,
            // reached through dispatch rather than as a local function.
            let method_file = Path::new(&unit.path)
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n.to_string_lossy().starts_with('@'));
            if let Some(f) = unit.find(name) {
                if !(method_file && Arc::ptr_eq(&f, &unit.functions[0])) {
                    return self.call_user(&unit, &f, args, nargout);
                }
            }
            if let Some(p) = self.private_function(&unit.path, name) {
                return self.call_file(&p, args, nargout);
            }
        }
        if let Some(m) = self.dispatch_method(name, &args) {
            return self.call_method(&m, args, nargout);
        }
        if let Some(e) = self.broken_method(name, &args) {
            return Err(e);
        }
        if let Some(c) = self.classes.get(name).cloned() {
            return self.construct(&c, args, nargout);
        }
        if let Some(p) = self.find_file(name, false) {
            return self.call_file(&p, args, nargout);
        }
        let complex = if args.iter().any(Value::is_complex) {
            self.complex_builtins.get(name).copied()
        } else if args.iter().any(crate::builtins::control::is_lti) {
            self.lti_builtins.get(name).copied()
        } else {
            None
        };
        if let Some(b) = complex.or_else(|| self.builtins.get(name).copied()) {
            self.last_builtin = name.to_string();
            return b(self, args, nargout).map_err(|mut e| {
                if e.origin.is_none()
                    && e.stack.is_empty()
                    && !e.interrupt
                    && !e.identifier.starts_with("Beany:user")
                {
                    e.origin = Some(name.to_string());
                }
                e
            });
        }
        if let Some(p) = self.find_file(name, true) {
            return self.call_file(&p, args, nargout);
        }
        if let Some(c) = self.find_class(name)? {
            return self.construct(&c, args, nargout);
        }
        Err(MError::with_id(
            "Beany:undefinedFunction",
            format!("Unrecognized function or variable '{}'.", name),
        ))
    }

    /// `name.m` in the `private/` folder next to the calling file.
    fn private_function(&mut self, caller: &str, name: &str) -> Option<PathBuf> {
        if caller.is_empty() {
            return None;
        }
        let dir = Path::new(caller).parent()?.join("private");
        let names = self.private_fns.entry(dir.clone()).or_insert_with(|| {
            std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .filter_map(|e| {
                            let p = e.path();
                            (p.extension()? == "m")
                                .then(|| p.file_stem().map(|s| s.to_string_lossy().to_string()))?
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        names
            .contains(name)
            .then(|| dir.join(format!("{}.m", name)))
    }

    fn call_file(&mut self, p: &Path, args: Vec<Value>, nargout: usize) -> MResult<Vec<Value>> {
        let unit = self.load_unit(p)?;
        if unit.class.is_some() {
            let c = self.class_of_unit(&unit)?.unwrap();
            return self.construct(&c, args, nargout);
        }
        if unit.script.is_some() {
            if !args.is_empty() {
                return Err(merr!(
                    "Attempt to execute SCRIPT {} as a function.",
                    p.file_stem().unwrap_or_default().to_string_lossy()
                ));
            }
            self.run_script_unit(&unit)?;
            return Ok(vec![]);
        }
        let f = unit.functions[0].clone();
        self.call_user(&unit, &f, args, nargout)
    }

    pub fn call_user(
        &mut self,
        unit: &Arc<FileUnit>,
        f: &Arc<Function>,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        self.call_user_preset(unit, f, args, nargout, Vec::new())
    }

    /// Call a user function with some variables set before it runs (a constructor's
    /// output starts as the default object).
    pub fn call_user_preset(
        &mut self,
        unit: &Arc<FileUnit>,
        f: &Arc<Function>,
        args: Vec<Value>,
        nargout: usize,
        preset: Vec<(String, Value)>,
    ) -> MResult<Vec<Value>> {
        let has_varargin = f.params.last().is_some_and(|p| p == "varargin");
        let fixed = if has_varargin {
            f.params.len() - 1
        } else {
            f.params.len()
        };
        if args.len() > fixed && !has_varargin {
            return Err(merr!("Too many input arguments."));
        }
        let has_varargout = f.outputs.last().is_some_and(|p| p == "varargout");
        if nargout > f.outputs.len() && !has_varargout {
            return Err(merr!("Too many output arguments."));
        }
        if self.depth > 400 {
            return Err(merr!("Maximum recursion limit of 400 reached."));
        }
        let nargin = args.len();
        let mut frame = Frame {
            name: f.name.clone(),
            persist_key: format!("{}>{}", unit.path, f.name),
            unit: Some(unit.clone()),
            nargin,
            nargout,
            line: f.line,
            ..Default::default()
        };
        // A nested function called from its parent shares the parent's variables.
        let shares = f.parent.as_deref().is_some_and(|p| {
            self.frame().name == p
                && self
                    .frame()
                    .unit
                    .as_ref()
                    .is_some_and(|u| Arc::ptr_eq(u, unit))
        });
        if shares {
            for (k, v) in &self.frame().vars {
                if !f.params.contains(k) && !f.outputs.contains(k) {
                    frame.vars.insert(k.clone(), v.clone());
                }
            }
        }
        for (k, v) in preset {
            frame.vars.insert(k, v);
        }
        let mut it = args.into_iter();
        for p in f.params.iter().take(fixed) {
            match it.next() {
                Some(v) => {
                    if p != "~" {
                        frame.vars.insert(p.clone(), v);
                    }
                }
                None => break,
            }
        }
        if has_varargin {
            let rest: Vec<Value> = it.collect();
            frame.vars.insert("varargin".into(), Value::cell_row(rest));
        }
        self.frames.push(frame);
        self.depth += 1;
        let r = self.exec_block(&f.body);
        self.depth -= 1;
        let frame = self.frames.pop().unwrap();
        if shares {
            let parent = self.frame_mut();
            for (k, v) in &frame.vars {
                if !f.params.contains(k) && !f.outputs.contains(k) && parent.vars.contains_key(k) {
                    parent.vars.insert(k.clone(), v.clone());
                }
            }
        }
        if let Err(mut e) = r {
            if !e.interrupt {
                e.stack.push(format!("{} (line {})", f.name, frame.line));
            }
            return Err(e);
        }
        let want = nargout.max(1);
        let mut outs = Vec::new();
        for (i, o) in f.outputs.iter().enumerate() {
            if outs.len() >= want {
                break;
            }
            if o == "varargout" && i == f.outputs.len() - 1 {
                if let Some(Value::Cell(c)) = frame.vars.get("varargout") {
                    for v in &c.data {
                        if outs.len() >= want {
                            break;
                        }
                        outs.push(v.clone());
                    }
                }
                break;
            }
            let v = if frame.globals.contains(o) {
                self.globals.get(o).cloned()
            } else if frame.persist.contains(o) {
                self.persistents
                    .get(&frame.persist_key)
                    .and_then(|m| m.get(o))
                    .cloned()
            } else {
                frame.vars.get(o).cloned()
            };
            match v {
                Some(v) => outs.push(v),
                None => {
                    if i < nargout {
                        return Err(merr!("Output argument \"{}\" (and possibly others) not assigned a value in the execution with \"{}\" function.", o, f.name));
                    }
                    break;
                }
            }
        }
        Ok(outs)
    }

    pub fn call_handle(
        &mut self,
        fh: &FuncHandle,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        match fh {
            FuncHandle::Named(n) => self.call_named(n, args, nargout),
            FuncHandle::Anon {
                params,
                body,
                captured,
                unit,
                ..
            } => {
                if args.len() > params.len() && !params.last().is_some_and(|p| p == "varargin") {
                    return Err(merr!("Too many input arguments."));
                }
                let mut frame = Frame {
                    name: "@anonymous".into(),
                    unit: unit.clone(),
                    nargin: args.len(),
                    nargout,
                    line: self.frame().line,
                    ..Default::default()
                };
                for (k, v) in captured.iter() {
                    frame.vars.insert(k.clone(), v.clone());
                }
                let mut it = args.into_iter();
                for p in params {
                    if p == "varargin" {
                        frame
                            .vars
                            .insert(p.clone(), Value::cell_row(it.by_ref().collect()));
                        break;
                    }
                    match it.next() {
                        Some(v) => {
                            if p != "~" {
                                frame.vars.insert(p.clone(), v);
                            }
                        }
                        None => break,
                    }
                }
                if self.depth > 400 {
                    return Err(merr!("Maximum recursion limit of 400 reached."));
                }
                self.frames.push(frame);
                self.depth += 1;
                let r = self.eval_multi(body, nargout);
                self.depth -= 1;
                self.frames.pop();
                r
            }
        }
    }

    /// Call a function value given as a handle, a name, or a char/string.
    pub fn call_value(
        &mut self,
        f: &Value,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        match f {
            Value::Func(fh) => self.call_handle(fh, args, nargout),
            other => {
                let name = other.text()?;
                self.call_named(&name, args, nargout)
            }
        }
    }

    pub fn call1(&mut self, f: &Value, args: Vec<Value>) -> MResult<Value> {
        let mut v = self.call_value(f, args, 1)?;
        if v.is_empty() {
            return Err(merr!("Function did not return a value."));
        }
        Ok(v.swap_remove(0))
    }

    pub fn reset_workspace(&mut self) {
        self.frames.truncate(1);
        self.frames[0].vars.clear();
        self.frames[0].globals.clear();
    }
}

/// Absolute folder path with `.`/`..` resolved, keeping the form the user gave
/// (mapped drive letters stay `Z:\...`). `fs::canonicalize` is avoided because on
/// Windows it rewrites mapped network drives to `\\?\UNC\server\share`.
pub fn clean_path(base: &Path, p: &str) -> PathBuf {
    use std::path::Component;
    // `join` keeps `p` as-is when it is already absolute.
    let joined = base.join(p);
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never pop past the drive/share root.
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    // A caller may still hand us a verbatim path; turn it back into a normal one.
    let s = out.to_string_lossy().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", rest));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    out
}

fn is_call_expr(e: &Expr, it: &Interp) -> bool {
    match e {
        Expr::Ident(n) => !it.is_var(n),
        Expr::Index(b, _) => match b.as_ref() {
            Expr::Ident(n) => !it.is_var(n) || matches!(it.get_var(n), Some(Value::Func(_))),
            _ => false,
        },
        _ => false,
    }
}

fn logical_scalar(v: &Value, op: &str) -> MResult<bool> {
    match v {
        Value::Num(a, _) if a.numel() == 1 => {
            if a.data[0].is_nan() {
                return Err(merr!("NaN's cannot be converted to logicals."));
            }
            Ok(a.data[0] != 0.0)
        }
        Value::Num(a, _) if a.is_empty() => Err(merr!("Invalid operand. The operands of the logical operator {} must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.", op)),
        _ => {
            if v.numel() == 1 {
                v.is_true()
            } else {
                Err(merr!("Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values."))
            }
        }
    }
}

fn switch_match(subj: &Value, c: &Value) -> bool {
    match c {
        Value::Cell(cells) => cells.data.iter().any(|x| switch_match(subj, x)),
        _ => {
            if subj.is_text() || c.is_text() {
                match (subj.text(), c.text()) {
                    (Ok(a), Ok(b)) => subj.is_text() && c.is_text() && a == b,
                    _ => false,
                }
            } else {
                match (subj.to_num(), c.to_num()) {
                    (Ok(a), Ok(b)) => a.numel() == 1 && b.numel() == 1 && a.data[0] == b.data[0],
                    _ => false,
                }
            }
        }
    }
}

fn field_values(v: &Value, name: &str) -> MResult<Vec<Value>> {
    match v {
        Value::Struct(s) => {
            let Some(k) = s.key_index(name) else {
                return Err(MError::with_id(
                    "Beany:nonExistentField",
                    format!("Unrecognized field name \"{}\".", name),
                ));
            };
            Ok(s.arr.data.iter().map(|e| e[k].clone()).collect())
        }
        Value::Obj(o) => Ok(vec![crate::builtins::objects::get_field(o, name)?]),
        _ => Err(merr!(
            "Dot indexing is not supported for variables of this type."
        )),
    }
}

pub fn exception_value(e: &MError) -> Value {
    let stack: Vec<Value> = e.stack.iter().map(|s| Value::chars(s)).collect();
    Value::Struct(StructArr::scalar(vec![
        ("identifier".into(), Value::chars(&e.identifier)),
        ("message".into(), Value::chars(&e.message)),
        ("stack".into(), Value::Cell(Array::col(stack))),
    ]))
}

fn empty_like(rhs: &Value) -> Value {
    match rhs {
        Value::Num(_, c) => Value::Num(Array::empty(), *c),
        Value::Char(_) => Value::Char(Array::empty()),
        Value::Str(_) => Value::Str(Array::empty()),
        Value::Cell(_) => Value::Cell(Array::empty()),
        Value::Struct(s) => Value::Struct(StructArr {
            keys: s.keys.clone(),
            arr: Array::empty(),
        }),
        Value::Func(_) | Value::Obj(_) | Value::Inst(_) | Value::Complex(_) => Value::empty(),
    }
}

/// `base(idx) = rhs` including deletion and type coercion.
fn paren_assign(base: Value, idx: &[Idx], rhs: Value) -> MResult<Value> {
    let is_delete = matches!(&rhs, Value::Num(a, NumClass::Double) if a.rows == 0 && a.cols == 0);
    if is_delete {
        let mut base = base;
        match &mut base {
            Value::Num(a, _) => a.delete(idx)?,
            Value::Char(a) => a.delete(idx)?,
            Value::Str(a) => a.delete(idx)?,
            Value::Cell(a) => a.delete(idx)?,
            Value::Struct(s) => s.arr.delete(idx)?,
            Value::Func(_) => return Err(merr!("Cannot delete from a function handle")),
            Value::Obj(o) => {
                return Err(merr!(
                    "Deleting elements of a {} is not supported yet.",
                    o.class
                ))
            }
            Value::Inst(o) => {
                let mut s = o.props();
                s.arr.delete(idx)?;
                return Ok(Value::Inst(Box::new(o.with_props(s))));
            }
            Value::Complex(a) => {
                a.delete(idx)?;
                return Ok(Value::complex(a.clone()));
            }
        }
        return Ok(base);
    }
    let base_empty = base.is_empty();
    Ok(match (base, rhs) {
        (Value::Num(mut a, c), Value::Num(b, rc)) => {
            let class = match (c, rc) {
                (NumClass::Int(k), _) => NumClass::Int(k),
                _ if base_empty => rc,
                (NumClass::Logical, NumClass::Logical) => NumClass::Logical,
                (NumClass::Logical, other) => match other {
                    NumClass::Int(k) => NumClass::Int(k),
                    NumClass::Single => NumClass::Single,
                    _ => NumClass::Double,
                },
                (cl, _) => cl,
            };
            let b = match class {
                NumClass::Int(k) => b.map(|x| k.cast(*x)),
                _ => b,
            };
            a.assign(idx, &b, &0.0)?;
            Value::Num(a, class)
        }
        (Value::Num(a, _), Value::Char(b)) if base_empty => {
            let mut c: Array<char> = Array::new(a.rows, a.cols, vec![]);
            c.assign(idx, &b, &'\0')?;
            Value::Char(c)
        }
        (Value::Num(mut a, c), Value::Char(b)) => {
            a.assign(idx, &b.map(|ch| *ch as u32 as f64), &0.0)?;
            Value::Num(a, c)
        }
        (Value::Char(mut a), Value::Char(b)) => {
            a.assign(idx, &b, &'\0')?;
            Value::Char(a)
        }
        (Value::Char(mut a), Value::Num(b, _)) => {
            a.assign(
                idx,
                &b.map(|x| char::from_u32(*x as u32).unwrap_or('\0')),
                &'\0',
            )?;
            Value::Char(a)
        }
        (Value::Cell(mut a), Value::Cell(b)) => {
            a.assign(idx, &b, &Value::empty())?;
            Value::Cell(a)
        }
        (Value::Num(..), Value::Cell(b)) if base_empty => {
            let mut a = Array::empty();
            a.assign(idx, &b, &Value::empty())?;
            Value::Cell(a)
        }
        (Value::Cell(_), other) => {
            return Err(merr!(
                "Conversion to cell from {} is not possible.",
                other.class_name()
            ));
        }
        (Value::Str(mut a), r @ (Value::Str(_) | Value::Char(_) | Value::Num(..))) => {
            let b = match r {
                Value::Str(b) => b,
                Value::Char(_) => Array::scalar(r.text()?),
                Value::Num(n, _) => n.map(|x| display::num_to_str(*x)),
                _ => unreachable!(),
            };
            a.assign(idx, &b, &String::new())?;
            Value::Str(a)
        }
        (Value::Num(..), Value::Str(b)) if base_empty => {
            let mut a = Array::empty();
            a.assign(idx, &b, &String::new())?;
            Value::Str(a)
        }
        (Value::Struct(mut s), Value::Struct(r)) => {
            for k in &r.keys {
                s.add_key(k);
            }
            if s.keys.len() != r.keys.len() && !s.arr.is_empty() {
                return Err(merr!(
                    "Subscripted assignment between dissimilar structures."
                ));
            }
            let src = r.arr.map(|e| {
                s.keys
                    .iter()
                    .map(|k| {
                        r.key_index(k)
                            .map(|i| e[i].clone())
                            .unwrap_or_else(Value::empty)
                    })
                    .collect::<Vec<Value>>()
            });
            let fill = s.empty_elem();
            s.arr.assign(idx, &src, &fill)?;
            Value::Struct(s)
        }
        (Value::Num(..), Value::Struct(r)) if base_empty => {
            let mut s = StructArr {
                keys: r.keys.clone(),
                arr: Array::empty(),
            };
            let fill = s.empty_elem();
            s.arr.assign(idx, &r.arr, &fill)?;
            Value::Struct(s)
        }
        (Value::Num(..), f @ Value::Func(_)) if base_empty => f,
        (b @ (Value::Num(..) | Value::Complex(_)), r @ (Value::Num(..) | Value::Complex(_)))
            if b.is_complex() || r.is_complex() =>
        {
            let mut a = b.to_cplx()?;
            let v = r.to_cplx()?;
            a.assign(idx, &v, &C64::new(0.0, 0.0))?;
            Value::complex(a)
        }
        (Value::Inst(a), Value::Inst(b)) if Arc::ptr_eq(&a.class, &b.class) && !a.is_handle() => {
            let mut s = a.props();
            let r = b.props();
            let src = r.arr.map(|e| {
                s.keys
                    .iter()
                    .map(|k| {
                        r.key_index(k)
                            .map(|i| e[i].clone())
                            .unwrap_or_else(Value::empty)
                    })
                    .collect::<Vec<Value>>()
            });
            let fill = s.empty_elem();
            s.arr.assign(idx, &src, &fill)?;
            Value::Inst(Box::new(a.with_props(s)))
        }
        (Value::Num(..), Value::Inst(b)) if base_empty && !b.is_handle() => {
            let mut s = b.props();
            let src = s.arr.clone();
            let fill = s.empty_elem();
            s.arr = Array::empty();
            s.arr.assign(idx, &src, &fill)?;
            Value::Inst(Box::new(b.with_props(s)))
        }
        (b, r) => {
            return Err(merr!(
                "Conversion to {} from {} is not possible.",
                b.class_name(),
                r.class_name()
            ))
        }
    })
}

pub fn collect_idents(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Ident(n) => {
            out.insert(n.clone());
        }
        Expr::Unary(_, a) | Expr::Transpose(a) | Expr::Paren(a) => collect_idents(a, out),
        Expr::Binary(_, a, b) => {
            collect_idents(a, out);
            collect_idents(b, out);
        }
        Expr::Range(a, s, b) => {
            collect_idents(a, out);
            if let Some(s) = s {
                collect_idents(s, out);
            }
            collect_idents(b, out);
        }
        Expr::Index(b, args) | Expr::CellIndex(b, args) => {
            collect_idents(b, out);
            for a in args {
                collect_idents(a, out);
            }
        }
        Expr::Field(b, _) => collect_idents(b, out),
        Expr::DynField(b, f) => {
            collect_idents(b, out);
            collect_idents(f, out);
        }
        Expr::Matrix(rows) | Expr::Cell(rows) => {
            for r in rows {
                for x in r {
                    collect_idents(x, out);
                }
            }
        }
        Expr::AnonFn { body, .. } => collect_idents(body, out),
        _ => {}
    }
}
