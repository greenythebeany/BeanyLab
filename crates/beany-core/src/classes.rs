//! User-defined classes: `classdef` files and `@folder` class directories.
//!
//! A class is loaded once into a [`ClassInfo`] (properties, methods, superclasses)
//! and cached by name. Objects are [`Value::Inst`] values; value classes carry their
//! properties, handle classes share them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::ast::{BinOp, ClassDef, FileUnit, Function, PropDef};
use crate::error::MResult;
use crate::interp::{Frame, Interp};
use crate::merr;
use crate::value::*;

#[derive(Clone)]
pub struct Method {
    pub unit: Arc<FileUnit>,
    pub func: Arc<Function>,
    pub is_static: bool,
    /// Class that defines the method.
    pub owner: &'static str,
}

pub struct ClassInfo {
    /// Interned for the life of the program, so `class_name()` can stay `&'static str`.
    pub name: &'static str,
    pub def: Option<Arc<ClassDef>>,
    /// The file holding the class definition (or the constructor of an old-style class).
    pub unit: Option<Arc<FileUnit>>,
    pub supers: Vec<Arc<ClassInfo>>,
    pub handle: bool,
    /// Own and inherited properties; inherited ones come first.
    pub props: Vec<PropDef>,
    /// Own and inherited methods; own methods override inherited ones.
    pub methods: HashMap<String, Method>,
    pub constructor: Option<Method>,
    /// Method files that failed to load, with the error, reported when called.
    pub broken: HashMap<String, String>,
}

impl std::fmt::Debug for ClassInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "class {}", self.name)
    }
}

impl ClassInfo {
    pub fn has_prop(&self, name: &str) -> bool {
        self.props.iter().any(|p| p.name == name)
    }
    pub fn prop(&self, name: &str) -> Option<&PropDef> {
        self.props.iter().find(|p| p.name == name)
    }
    /// The class or one of its ancestors is called `name`.
    pub fn isa(&self, name: &str) -> bool {
        self.name == name
            || (name == "handle" && self.handle)
            || self.supers.iter().any(|s| s.isa(name))
    }
}

fn intern(s: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut set = NAMES.get_or_init(Default::default).lock().unwrap();
    if let Some(n) = set.get(s) {
        return n;
    }
    let n: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(n);
    n
}

/// Method name that overloads a binary operator.
pub fn binop_method(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "plus",
        BinOp::Sub => "minus",
        BinOp::MatMul => "mtimes",
        BinOp::ElMul => "times",
        BinOp::MatDiv => "mrdivide",
        BinOp::ElDiv => "rdivide",
        BinOp::MatLDiv => "mldivide",
        BinOp::ElLDiv => "ldivide",
        BinOp::MatPow => "mpower",
        BinOp::ElPow => "power",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And | BinOp::AndAnd => "and",
        BinOp::Or | BinOp::OrOr => "or",
    }
}

impl Interp {
    /// Find and load the class `name` from the current folder or the path.
    pub fn find_class(&mut self, name: &str) -> MResult<Option<Arc<ClassInfo>>> {
        if let Some(c) = self.classes.get(name) {
            return Ok(Some(c.clone()));
        }
        if name.is_empty() || name.contains(['/', '\\', '.']) {
            return Ok(None);
        }
        let mut dirs = vec![self.cwd.clone()];
        dirs.extend(self.path.iter().cloned());
        for d in dirs {
            let at = d.join(format!("@{}", name));
            if at.is_dir() {
                let file = at.join(format!("{}.m", name));
                let unit = if file.is_file() {
                    Some(self.load_unit(&file)?)
                } else {
                    None
                };
                return self.register_class(name, unit, Some(at)).map(Some);
            }
            let file = d.join(format!("{}.m", name));
            if file.is_file() {
                let unit = self.load_unit(&file)?;
                if unit.class.is_some() {
                    return self.register_class(name, Some(unit), None).map(Some);
                }
            }
        }
        Ok(None)
    }

    /// The class defined by a loaded file, if it is a class file.
    pub fn class_of_unit(&mut self, unit: &Arc<FileUnit>) -> MResult<Option<Arc<ClassInfo>>> {
        let Some(def) = &unit.class else {
            return Ok(None);
        };
        if let Some(c) = self.classes.get(def.name.as_str()) {
            if c.unit.as_ref().is_some_and(|u| Arc::ptr_eq(u, unit)) {
                return Ok(Some(c.clone()));
            }
        }
        let dir = Path::new(&unit.path)
            .parent()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy() == format!("@{}", def.name))
            })
            .map(Path::to_path_buf);
        self.register_class(&def.name.clone(), Some(unit.clone()), dir)
            .map(Some)
    }

    fn register_class(
        &mut self,
        name: &str,
        unit: Option<Arc<FileUnit>>,
        dir: Option<PathBuf>,
    ) -> MResult<Arc<ClassInfo>> {
        let def = unit.as_ref().and_then(|u| u.class.clone());
        let mut supers = Vec::new();
        let mut handle = false;
        if let Some(d) = &def {
            for s in &d.supers {
                match s.as_str() {
                    "handle"
                    | "dynamicprops"
                    | "matlab.mixin.Copyable"
                    | "hgsetget"
                    | "matlab.mixin.SetGet" => handle = true,
                    s if s.starts_with("matlab.") => {}
                    s => {
                        let sc = self.find_class(s)?.ok_or_else(|| {
                            merr!("Superclass '{}' of '{}' was not found.", s, name)
                        })?;
                        handle |= sc.handle;
                        supers.push(sc);
                    }
                }
            }
        }
        let mut props: Vec<PropDef> = Vec::new();
        let mut methods: HashMap<String, Method> = HashMap::new();
        for s in &supers {
            for p in &s.props {
                if !props.iter().any(|q| q.name == p.name) {
                    props.push(p.clone());
                }
            }
            for (k, m) in &s.methods {
                methods.entry(k.clone()).or_insert_with(|| m.clone());
            }
        }
        let cname = intern(name);
        let mut constructor = None;
        let mut broken = HashMap::new();
        if let (Some(d), Some(u)) = (&def, &unit) {
            for p in &d.props {
                props.retain(|q| q.name != p.name);
                props.push(p.clone());
            }
            for m in &d.methods {
                let method = Method {
                    unit: u.clone(),
                    func: m.func.clone(),
                    is_static: m.is_static,
                    owner: cname,
                };
                if m.func.name == name {
                    constructor = Some(method);
                } else {
                    methods.insert(m.func.name.clone(), method);
                }
            }
        } else if let Some(u) = &unit {
            // Old-style class: the constructor is a plain function.
            if let Some(f) = u.functions.first() {
                constructor = Some(Method {
                    unit: u.clone(),
                    func: f.clone(),
                    is_static: false,
                    owner: cname,
                });
            }
        }
        // Methods in their own files inside the @folder.
        if let Some(dir) = &dir {
            let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|rd| rd.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            files.sort();
            for f in files {
                let Some(stem) = f.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                    continue;
                };
                if stem == name || f.extension().is_none_or(|e| e != "m") {
                    continue;
                }
                let u = match self.load_unit(&f) {
                    Ok(u) => u,
                    Err(e) => {
                        broken.insert(stem, e.message);
                        continue;
                    }
                };
                if let Some(func) = u.functions.first().cloned() {
                    let is_static = def
                        .as_ref()
                        .and_then(|d| d.methods.iter().find(|m| m.func.name == stem))
                        .is_some_and(|m| m.is_static);
                    methods.insert(
                        stem,
                        Method {
                            unit: u,
                            func,
                            is_static,
                            owner: cname,
                        },
                    );
                }
            }
        }
        let info = Arc::new(ClassInfo {
            name: cname,
            def,
            unit,
            supers,
            handle,
            props,
            methods,
            constructor,
            broken,
        });
        self.classes.insert(name.to_string(), info.clone());
        Ok(info)
    }

    /// An object with every property at its default value.
    pub fn default_object(&mut self, class: &Arc<ClassInfo>) -> MResult<Value> {
        let mut fields = Vec::new();
        for p in class.props.clone() {
            if p.constant || p.dependent {
                continue;
            }
            let v = match &p.default {
                Some(e) => self.eval_in_class(class, e)?,
                None => Value::empty(),
            };
            fields.push((p.name.clone(), v));
        }
        let s = StructArr::scalar(fields);
        let data = if class.handle {
            InstData::Handle(Arc::new(Mutex::new(s)))
        } else {
            InstData::Value(s)
        };
        Ok(Value::Inst(Box::new(Instance {
            class: class.clone(),
            data,
        })))
    }

    /// Evaluate a property default or constant in the class file's context.
    fn eval_in_class(&mut self, class: &Arc<ClassInfo>, e: &crate::ast::Expr) -> MResult<Value> {
        self.frames.push(Frame {
            name: class.name.to_string(),
            unit: class.unit.clone(),
            ..Default::default()
        });
        let r = self.eval(e);
        self.frames.pop();
        r
    }

    /// Call the class constructor (or build a default object when there is none).
    pub fn construct(
        &mut self,
        class: &Arc<ClassInfo>,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        let obj = if class.def.is_some() {
            self.default_object(class)?
        } else {
            Value::empty()
        };
        match class.constructor.clone() {
            Some(m) => {
                let preset = match (class.def.is_some(), m.func.outputs.first()) {
                    (true, Some(o)) => vec![(o.clone(), obj)],
                    _ => Vec::new(),
                };
                let mut out =
                    self.call_user_preset(&m.unit, &m.func, args, nargout.max(1), preset)?;
                out.truncate(nargout.max(1));
                Ok(out)
            }
            None if args.is_empty() => Ok(vec![obj]),
            None => Err(merr!("Too many input arguments.")),
        }
    }

    /// `name@Super(args)` inside a constructor or method.
    pub fn super_call(
        &mut self,
        name: &str,
        sup: &str,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        let sc = self
            .find_class(sup)?
            .ok_or_else(|| merr!("Superclass '{}' was not found.", sup))?;
        // Constructor form: the current object is passed in and comes back initialized.
        if let Some(Value::Inst(_)) = self.get_var(name).cloned() {
            if self.frame().name == self.current_class_ctor() {
                let obj = self.get_var(name).cloned().unwrap();
                let Some(m) = sc.constructor.clone() else {
                    return Ok(vec![obj]);
                };
                let o = m
                    .func
                    .outputs
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "obj".into());
                return self.call_user_preset(&m.unit, &m.func, args, 1, vec![(o, obj)]);
            }
        }
        let m = sc
            .methods
            .get(name)
            .cloned()
            .ok_or_else(|| merr!("No method '{}' in superclass '{}'.", name, sup))?;
        self.call_user(&m.unit, &m.func, args, nargout)
    }

    /// Name of the function in the current frame when it is a class constructor.
    fn current_class_ctor(&self) -> String {
        match self.frame().unit.as_ref().and_then(|u| u.class.as_ref()) {
            Some(d) => d.name.clone(),
            None => String::new(),
        }
    }

    /// The method `name` of the first object argument that defines it.
    pub fn dispatch_method(&self, name: &str, args: &[Value]) -> Option<Method> {
        for a in args {
            if let Value::Inst(o) = a {
                if let Some(m) = o.class.methods.get(name) {
                    return Some(m.clone());
                }
            }
        }
        None
    }

    /// Error for a method of an object argument whose file could not be loaded.
    pub fn broken_method(&self, name: &str, args: &[Value]) -> Option<crate::error::MError> {
        args.iter().find_map(|a| match a {
            Value::Inst(o) => o.class.broken.get(name).map(|e| merr!("{}", e)),
            _ => None,
        })
    }

    pub fn call_method(
        &mut self,
        m: &Method,
        args: Vec<Value>,
        nargout: usize,
    ) -> MResult<Vec<Value>> {
        self.call_user(&m.unit, &m.func, args, nargout)
    }

    /// Apply an overloaded operator method to object operands.
    pub fn object_operator(&mut self, method: &str, sym: &str, args: Vec<Value>) -> MResult<Value> {
        let Some(m) = self.dispatch_method(method, &args) else {
            let cls = args
                .iter()
                .find(|a| matches!(a, Value::Inst(_)))
                .map(|a| a.class_name())
                .unwrap_or("object");
            return Err(merr!(
                "Operator '{}' is not supported for operands of type '{}'.",
                sym,
                cls
            ));
        };
        let mut out = self.call_method(&m, args, 1)?;
        if out.is_empty() {
            return Err(merr!("Too many output arguments."));
        }
        Ok(out.remove(0))
    }

    /// Join objects of one value class into an object array (the default `[a b]`).
    pub fn concat_objects(&mut self, items: Vec<Value>, horizontal: bool) -> MResult<Value> {
        let method = if horizontal { "horzcat" } else { "vertcat" };
        if let Some(m) = self.dispatch_method(method, &items) {
            let mut out = self.call_method(&m, items, 1)?;
            return out.pop().ok_or_else(|| merr!("Too many output arguments."));
        }
        let mut first: Option<Box<Instance>> = None;
        let mut parts = Vec::new();
        for v in items {
            match v {
                Value::Inst(o) => {
                    if let Some(f) = &first {
                        if !Arc::ptr_eq(&f.class, &o.class) {
                            return Err(merr!("Cannot concatenate objects of different classes."));
                        }
                    }
                    parts.push(o.props());
                    first.get_or_insert(o);
                }
                v if v.is_empty() => {}
                v => {
                    return Err(merr!(
                        "Cannot concatenate {} with an object.",
                        v.class_name()
                    ))
                }
            }
        }
        let first = first.ok_or_else(|| merr!("Nothing to concatenate."))?;
        if first.is_handle() && parts.len() > 1 {
            return Err(merr!("Arrays of handle objects are not supported yet."));
        }
        let keys = parts[0].keys.clone();
        let mut data = Vec::new();
        let (mut rows, mut cols) = (0, 0);
        for p in &parts {
            if p.keys != keys {
                return Err(merr!(
                    "Cannot concatenate objects with different properties."
                ));
            }
            data.extend(p.arr.data.iter().cloned());
            if horizontal {
                rows = p.arr.rows.max(rows);
                cols += p.arr.cols;
            } else {
                cols = p.arr.cols.max(cols);
                rows += p.arr.rows;
            }
        }
        if rows * cols != data.len() {
            return Err(merr!(
                "Dimensions of arrays being concatenated are not consistent."
            ));
        }
        let arr = if horizontal {
            Array::new(rows, cols, data)
        } else {
            // Stack row-wise: rebuild column-major order.
            let mut d = Vec::with_capacity(data.len());
            for c in 0..cols {
                let mut r0 = 0;
                for p in &parts {
                    for r in 0..p.arr.rows {
                        d.push(p.arr.data[c * p.arr.rows + r].clone());
                    }
                    r0 += p.arr.rows;
                }
                let _ = r0;
            }
            Array::new(rows, cols, d)
        };
        Ok(Value::Inst(Box::new(
            first.with_props(StructArr { keys, arr }),
        )))
    }

    /// `obj.name` for a property (through a `get.` method when there is one).
    pub fn get_prop(&mut self, o: &Instance, name: &str) -> MResult<Vec<Value>> {
        if let Some(p) = o.class.prop(name).cloned() {
            let getter = format!("get.{}", name);
            if let Some(m) = o.class.methods.get(&getter).cloned() {
                if self.frame().name != getter {
                    return self.call_method(&m, vec![Value::Inst(Box::new(o.clone()))], 1);
                }
            }
            if p.constant {
                let e = p.default.clone().unwrap_or(crate::ast::Expr::Num(0.0));
                return Ok(vec![self.eval_in_class(&o.class, &e)?]);
            }
            let s = o.props();
            if let Some(k) = s.key_index(name) {
                return Ok(s.arr.data.iter().map(|e| e[k].clone()).collect());
            }
            return Ok(vec![Value::empty()]);
        }
        Err(merr!(
            "Unrecognized method, property, or field '{}' for class '{}'.",
            name,
            o.class.name
        ))
    }

    /// `obj.name = v`, returning the updated object (through a `set.` method when
    /// there is one).
    pub fn set_prop(&mut self, mut o: Box<Instance>, name: &str, v: Value) -> MResult<Value> {
        if !o.class.has_prop(name) {
            return Err(merr!(
                "Unrecognized property '{}' for class '{}'.",
                name,
                o.class.name
            ));
        }
        let setter = format!("set.{}", name);
        if let Some(m) = o.class.methods.get(&setter).cloned() {
            if self.frame().name != setter {
                let mut out = self.call_method(&m, vec![Value::Inst(o.clone()), v], 1)?;
                return Ok(if o.is_handle() || out.is_empty() {
                    Value::Inst(o)
                } else {
                    out.remove(0)
                });
            }
        }
        match &mut o.data {
            InstData::Value(s) => {
                let k = s.add_key(name);
                for e in s.arr.data.iter_mut() {
                    e[k] = v.clone();
                }
            }
            InstData::Handle(h) => {
                let mut s = h.lock().unwrap();
                let k = s.add_key(name);
                for e in s.arr.data.iter_mut() {
                    e[k] = v.clone();
                }
            }
        }
        Ok(Value::Inst(o))
    }

    /// `Class.member`: a static method or a constant property, when `Class` is a class.
    pub fn class_member(&mut self, class: &str, member: &str) -> MResult<Option<ClassMember>> {
        let Some(c) = self.find_class(class)? else {
            return Ok(None);
        };
        if let Some(m) = c.methods.get(member) {
            return Ok(Some(ClassMember::Method(m.clone())));
        }
        if let Some(p) = c.prop(member).cloned() {
            if p.constant {
                let e = p.default.clone().unwrap_or(crate::ast::Expr::Num(0.0));
                return Ok(Some(ClassMember::Value(self.eval_in_class(&c, &e)?)));
            }
        }
        Ok(None)
    }
}

pub enum ClassMember {
    Method(Method),
    Value(Value),
}
