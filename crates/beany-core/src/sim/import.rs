//! Reading models saved by other block-diagram tools: `.slx` (a zip package of XML
//! parts) and the older text `.mdl` format. Blocks are mapped onto the BeanyLab
//! library; anything without an equivalent becomes a placeholder subsystem that keeps
//! the wiring intact and outputs zero, and is reported as a warning.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use super::model::{BlockSpec, Line, Model, PortRef, SolverSettings};

/// A block-diagram element: XML element or `.mdl` section, reduced to one shape.
#[derive(Debug, Clone, Default)]
struct Node {
    tag: String,
    attrs: BTreeMap<String, String>,
    /// Named parameters (`<P Name="...">` in XML, `Key value` lines in `.mdl`).
    p: Vec<(String, String)>,
    text: String,
    kids: Vec<Node>,
}

impl Node {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs
            .get(k)
            .map(String::as_str)
            .or_else(|| self.p.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()))
    }
    fn kids<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.kids.iter().filter(move |k| k.tag == tag)
    }
    fn kid(&self, tag: &str) -> Option<&Node> {
        self.kids.iter().find(|k| k.tag == tag)
    }
    /// First parameter `k` anywhere below this node.
    fn find_param(&self, k: &str) -> Option<&str> {
        if let Some(v) = self.p.iter().find(|(n, _)| n == k) {
            return Some(&v.1);
        }
        self.kids.iter().find_map(|c| c.find_param(k))
    }
}

// ---------------------------------------------------------------- readers

fn from_xml(text: &str) -> Result<Node, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("invalid XML: {}", e))?;
    fn walk(n: roxmltree::Node) -> Node {
        let mut out = Node {
            tag: n.tag_name().name().to_string(),
            ..Default::default()
        };
        for a in n.attributes() {
            out.attrs
                .insert(a.name().to_string(), a.value().to_string());
        }
        for c in n.children() {
            if c.is_text() {
                out.text.push_str(c.text().unwrap_or(""));
            } else if c.is_element() {
                if c.tag_name().name() == "P" {
                    let name = c.attribute("Name").unwrap_or("").to_string();
                    let value: String = c.children().filter_map(|t| t.text()).collect();
                    out.p.push((name, value));
                } else {
                    out.kids.push(walk(c));
                }
            }
        }
        out.text = out.text.trim().to_string();
        out
    }
    Ok(walk(doc.root_element()))
}

/// JSON parts of newer packages: objects become nodes, scalar members parameters.
fn from_json(text: &str) -> Result<Node, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON: {}", e))?;
    fn walk(tag: &str, v: &serde_json::Value) -> Node {
        let mut out = Node {
            tag: tag.to_string(),
            ..Default::default()
        };
        if let serde_json::Value::Object(m) = v {
            for (k, x) in m {
                match x {
                    serde_json::Value::String(s) => out.p.push((k.clone(), s.clone())),
                    serde_json::Value::Number(n) => out.p.push((k.clone(), n.to_string())),
                    serde_json::Value::Bool(b) => out
                        .p
                        .push((k.clone(), if *b { "on" } else { "off" }.into())),
                    serde_json::Value::Array(items) => {
                        for i in items {
                            out.kids.push(walk(k, i));
                        }
                    }
                    serde_json::Value::Object(_) => out.kids.push(walk(k, x)),
                    serde_json::Value::Null => {}
                }
            }
        }
        out
    }
    Ok(walk("root", &v))
}

/// Parse the text `.mdl` format: `Name { Key value ... Child { ... } }`.
fn from_mdl(text: &str) -> Result<Node, String> {
    let mut stack: Vec<Node> = vec![Node {
        tag: "File".into(),
        ..Default::default()
    }];
    let mut lines = text.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "}" {
            let done = stack.pop().ok_or("unbalanced braces")?;
            stack.last_mut().ok_or("unbalanced braces")?.kids.push(done);
            continue;
        }
        // Embedded packages in newer .mdl files hold no diagram data we need.
        if line.starts_with("__MWOPC_PACKAGE_BEGIN__") {
            break;
        }
        let (key, rest) = match line.find(char::is_whitespace) {
            Some(i) => (&line[..i], line[i..].trim()),
            None => (line, ""),
        };
        if rest == "{" {
            stack.push(Node {
                tag: key.to_string(),
                ..Default::default()
            });
            continue;
        }
        let mut value = unquote(rest);
        // A quoted value may continue on following lines that start with a quote.
        if rest.starts_with('"') {
            while let Some(next) = lines.peek() {
                let t = next.trim();
                if t.starts_with('"') {
                    value.push_str(&unquote(t));
                    lines.next();
                } else {
                    break;
                }
            }
        }
        if let Some(top) = stack.last_mut() {
            top.p.push((key.to_string(), value));
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().unwrap();
        stack.last_mut().unwrap().kids.push(done);
    }
    Ok(stack.pop().unwrap())
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if !(s.starts_with('"') && s.ends_with('"') && s.len() >= 2) {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- packages

struct Source {
    root: Node,
    /// `.slx`: separately stored systems by reference ("system_18").
    systems: HashMap<String, Node>,
    defaults: HashMap<String, Vec<(String, String)>>,
    config: Option<Node>,
    from_mdl: bool,
}

/// Read an `.slx` or `.mdl` file into a BeanyLab model. Returns the model and a list of
/// warnings about anything that could not be carried over exactly.
pub fn import_file(path: &std::path::Path) -> Result<(Model, Vec<String>), String> {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".into());
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let src = if bytes.starts_with(b"PK") {
        read_slx(&bytes)?
    } else {
        let text = String::from_utf8_lossy(&bytes);
        read_mdl(&text)?
    };
    Ok(convert(&src, &name))
}

fn read_slx(bytes: &[u8]) -> Result<Source, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not a valid model package: {}", e))?;
    let mut parts: HashMap<String, String> = HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let n = f.name().to_string();
        if n.ends_with(".xml") || n.ends_with(".json") {
            let mut s = String::new();
            if f.read_to_string(&mut s).is_ok() {
                parts.insert(n, s);
            }
        }
    }
    // The diagram's settings are in blockdiagram.xml, or blockDiagram.json in newer files.
    let info = if let Some(bd) = parts.get("simulink/blockdiagram.xml") {
        from_xml(bd)?
    } else if let Some(bd) = parts.get("simulink/blockDiagram.json") {
        from_json(bd)?
    } else {
        return Err("the package has no block diagram part".into());
    };
    let root = info
        .kid("Model")
        .or_else(|| info.kid("Library"))
        .cloned()
        .unwrap_or(info);
    let mut systems = HashMap::new();
    for (n, text) in &parts {
        if let Some(rest) = n.strip_prefix("simulink/systems/") {
            if !rest.contains('/') {
                systems.insert(rest.trim_end_matches(".xml").to_string(), from_xml(text)?);
            }
        }
    }
    let mut defaults = HashMap::new();
    let def_node = match parts.get("simulink/bddefaults.xml") {
        Some(t) => Some(from_xml(t)?),
        None => None,
    };
    for holder in def_node.iter().chain(std::iter::once(&root)) {
        if let Some(bpd) = holder.kid("BlockParameterDefaults") {
            for b in bpd.kids("Block") {
                if let Some(t) = b.get("BlockType") {
                    defaults.insert(t.to_string(), b.p.clone());
                }
            }
        }
    }
    // Several configurations may be stored; configSetInfo names the active one.
    let active = parts
        .get("simulink/configSetInfo.xml")
        .and_then(|t| from_xml(t).ok())
        .and_then(|info| {
            info.kids("ConfigSet")
                .find(|c| c.get("Active") == Some("true"))
                .and_then(|c| c.get("PartName"))
                .map(|p| p.trim_start_matches('/').to_string())
        })
        .unwrap_or_else(|| "simulink/configSet0.xml".into());
    let active_json = active.replace(".xml", ".json");
    let config = match (parts.get(&active), parts.get(&active_json)) {
        (Some(t), _) => Some(from_xml(t)?),
        (None, Some(t)) => Some(from_json(t)?),
        _ => None,
    };
    Ok(Source {
        root,
        systems,
        defaults,
        config,
        from_mdl: false,
    })
}

fn read_mdl(text: &str) -> Result<Source, String> {
    let file = from_mdl(text)?;
    let root = file
        .kid("Model")
        .or_else(|| file.kid("Library"))
        .cloned()
        .ok_or("no Model section in the file")?;
    let mut defaults = HashMap::new();
    if let Some(bpd) = root.kid("BlockParameterDefaults") {
        for b in bpd.kids("Block") {
            if let Some(t) = b.get("BlockType") {
                defaults.insert(t.to_string(), b.p.clone());
            }
        }
    }
    Ok(Source {
        root,
        systems: HashMap::new(),
        defaults,
        config: None,
        from_mdl: true,
    })
}

// ---------------------------------------------------------------- conversion

fn convert(src: &Source, name: &str) -> (Model, Vec<String>) {
    let mut warnings = Vec::new();
    let solver = solver_settings(src);
    // The top system is inline, a reference to a separate part, or the "system_root" part.
    let root_sys = match src.root.kid("System") {
        Some(sys) => match sys.get("Ref") {
            Some(r) => src.systems.get(r).cloned(),
            None => Some(sys.clone()),
        },
        None => None,
    }
    .or_else(|| src.systems.get("system_root").cloned())
    .unwrap_or_default();
    let (blocks, lines) = convert_system(src, &root_sys, name, &mut warnings);
    let model = Model {
        version: 1,
        name: name.to_string(),
        solver,
        blocks,
        lines,
    };
    (model, warnings)
}

fn solver_settings(src: &Source) -> SolverSettings {
    let look = |k: &str| -> Option<String> {
        src.config
            .as_ref()
            .and_then(|c| c.find_param(k))
            .or_else(|| src.root.find_param(k))
            .map(|s| s.trim().to_string())
    };
    let mut s = SolverSettings::default();
    if let Some(v) = look("StopTime") {
        s.stop_time = v;
    }
    if let Some(v) = look("SolverName").or_else(|| look("Solver")) {
        s.solver = match v.as_str() {
            "ode45" | "ode23" | "ode4" | "ode2" | "ode1" | "ode23s" => v.clone(),
            "ode15s" | "ode23t" | "ode23tb" | "ode15i" => "ode23s".into(),
            "ode113" => "ode45".into(),
            "ode3" | "ode5" | "ode8" | "ode14x" => "ode4".into(),
            "FixedStepDiscrete" | "VariableStepDiscrete" => "discrete".into(),
            _ => "auto".into(),
        };
    }
    for (k, slot) in [
        ("MaxStep", &mut s.max_step),
        ("FixedStep", &mut s.fixed_step),
        ("RelTol", &mut s.rel_tol),
        ("AbsTol", &mut s.abs_tol),
    ] {
        if let Some(v) = look(k) {
            *slot = v;
        }
    }
    s
}

fn nums(s: &str) -> Vec<f64> {
    s.trim_matches(|c| c == '[' || c == ']')
        .split([',', ' ', ';'])
        .filter_map(|x| x.trim().parse::<f64>().ok())
        .collect()
}

/// A block's parameter: its own value, its instance data, else the file's default for its type.
fn param(src: &Source, b: &Node, k: &str) -> Option<String> {
    if let Some(v) = b.p.iter().find(|(n, _)| n == k) {
        return Some(v.1.clone());
    }
    if let Some(v) = b.kid("InstanceData").and_then(|d| d.get(k)) {
        return Some(v.to_string());
    }
    let t = b.get("BlockType")?;
    src.defaults
        .get(t)
        .and_then(|d| d.iter().find(|(n, _)| n == k))
        .map(|(_, v)| v.clone())
}

fn clean_name(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_string()
}

/// Parse a port reference like "12#out:1" or "12#enable" into (block key, kind, number).
fn port_ref(s: &str) -> Option<(String, String, usize)> {
    let (blk, port) = s.split_once('#')?;
    let (kind, n) = match port.split_once(':') {
        Some((k, n)) => (k.to_string(), n.trim().parse().ok()?),
        None => (port.to_string(), 1),
    };
    Some((blk.trim().to_string(), kind, n))
}

/// Line endpoints, following branches: (source ref, destination refs, line name).
fn line_ends(
    l: &Node,
    from_mdl: bool,
) -> (
    Option<(String, String, usize)>,
    Vec<(String, String, usize)>,
) {
    let end = |n: &Node, which: &str| -> Option<(String, String, usize)> {
        if from_mdl {
            let blk = n.get(&format!("{}Block", which))?;
            let port = n.get(&format!("{}Port", which)).unwrap_or("1");
            match port.parse::<usize>() {
                Ok(k) => Some((
                    blk.to_string(),
                    if which == "Src" { "out" } else { "in" }.into(),
                    k,
                )),
                Err(_) => Some((blk.to_string(), port.to_string(), 1)),
            }
        } else {
            port_ref(n.get(which)?)
        }
    };
    fn collect(
        n: &Node,
        end: &dyn Fn(&Node, &str) -> Option<(String, String, usize)>,
        out: &mut Vec<(String, String, usize)>,
    ) {
        if let Some(d) = end(n, "Dst") {
            out.push(d);
        }
        for b in n.kids("Branch") {
            collect(b, end, out);
        }
    }
    let mut dsts = Vec::new();
    collect(l, &end, &mut dsts);
    (end(l, "Src"), dsts)
}

struct Converted {
    kind: String,
    params: BTreeMap<String, String>,
    system: Option<Model>,
}

fn convert_system(
    src: &Source,
    sys: &Node,
    path: &str,
    warnings: &mut Vec<String>,
) -> (Vec<BlockSpec>, Vec<Line>) {
    let mut blocks = Vec::new();
    // Block key used by lines (SID in .slx, name in .mdl) -> (our id, input count).
    let mut keys: HashMap<String, (String, usize)> = HashMap::new();
    let mut bus_creators: Vec<(String, String)> = Vec::new();
    // Goto tags that are local to this system (the default visibility).
    let mut local_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Highest input/output port each block's lines use, so port counts are right even
    // when the file leaves them implicit.
    let mut used: HashMap<String, (usize, usize)> = HashMap::new();
    for l in sys.kids("Line") {
        let (s, dsts) = line_ends(l, src.from_mdl);
        if let Some((k, kind, n)) = s {
            if kind == "out" {
                let e = used.entry(k).or_default();
                e.1 = e.1.max(n);
            }
        }
        for (k, kind, n) in dsts {
            if kind == "in" {
                let e = used.entry(k).or_default();
                e.0 = e.0.max(n);
            }
        }
    }
    for (i, b) in sys.kids("Block").enumerate() {
        let bt = b.get("BlockType").unwrap_or("").to_string();
        let name = clean_name(b.get("Name").unwrap_or(&bt));
        if param(src, b, "Commented").as_deref() == Some("on") {
            continue;
        }
        let key = if src.from_mdl {
            b.get("Name").unwrap_or("").to_string()
        } else {
            b.get("SID")
                .map(str::to_string)
                .unwrap_or_else(|| name.clone())
        };
        let id = format!("b{}", i + 1);
        let pos = param(src, b, "Position")
            .map(|s| nums(&s))
            .unwrap_or_default();
        let (x, y, w, h) = if pos.len() == 4 {
            (
                pos[0] * 2.0,
                pos[1] * 2.0,
                ((pos[2] - pos[0]) * 2.0).max(10.0),
                ((pos[3] - pos[1]) * 2.0).max(10.0),
            )
        } else {
            (i as f64 * 120.0, 100.0, 70.0, 50.0)
        };
        // Port counts: "Ports" [in, out] in older files, <PortCounts in= out=> in newer ones.
        let ports = param(src, b, "Ports").map(|s| nums(&s)).unwrap_or_default();
        let counted = b.kid("PortCounts");
        let declared = |i: usize, attr: &str| -> Option<usize> {
            ports.get(i).map(|v| *v as usize).or_else(|| {
                counted
                    .and_then(|c| c.get(attr))
                    .and_then(|v| v.parse().ok())
            })
        };
        let (ui, uo) = used.get(&key).copied().unwrap_or((0, 0));
        let n_in = declared(0, "in").unwrap_or(1).max(ui);
        let n_out = declared(1, "out").unwrap_or(1).max(uo);
        let full = format!("{}/{}", path, name);
        let c = convert_block(src, b, &bt, &full, n_in, n_out, warnings);
        if bt == "Goto" && param(src, b, "TagVisibility").as_deref() != Some("global") {
            local_tags.insert(c.params.get("tag").cloned().unwrap_or_default());
        }
        let inputs = if c.kind == "EnabledSubsystem" {
            n_in
        } else {
            usize::MAX
        };
        if c.kind == "BusCreator" {
            bus_creators.push((id.clone(), key.clone()));
        }
        keys.insert(key, (id.clone(), inputs));
        blocks.push(BlockSpec {
            id,
            kind: c.kind,
            name,
            x,
            y,
            w,
            h,
            params: c.params,
            system: c.system.map(Box::new),
        });
    }
    // Local tags only connect blocks in this system: make them unique to it.
    for b in blocks.iter_mut() {
        if b.kind == "Goto" || b.kind == "From" {
            if let Some(t) = b.params.get_mut("tag") {
                if local_tags.contains(t.as_str()) {
                    *t = format!("{}|{}", path, t);
                }
            }
        }
    }
    let mut lines = Vec::new();
    let mut taken = std::collections::HashSet::new();
    let mut bus_names: HashMap<(String, usize), String> = HashMap::new();
    for l in sys.kids("Line") {
        let (s, dsts) = line_ends(l, src.from_mdl);
        let Some((sk, skind, sn)) = s else { continue };
        let Some((sid, _)) = keys.get(&sk).cloned() else {
            continue;
        };
        if skind != "out" {
            warnings.push(format!(
                "{}: a line from a '{}' port was dropped.",
                path, skind
            ));
            continue;
        }
        for (dk, dkind, dn) in dsts {
            let Some((did, n_in)) = keys.get(&dk).cloned() else {
                continue;
            };
            let port = match dkind.as_str() {
                "in" => dn,
                "enable" if n_in != usize::MAX => n_in + 1,
                other => {
                    warnings.push(format!(
                        "{}: a line into a '{}' port was dropped (not supported yet).",
                        path, other
                    ));
                    continue;
                }
            };
            if !taken.insert((did.clone(), port)) {
                continue;
            }
            if let Some(n) = l.get("Name") {
                bus_names.insert((did.clone(), port), clean_name(n));
            }
            lines.push(Line {
                src: PortRef {
                    block: sid.clone(),
                    port: sn,
                },
                dst: PortRef { block: did, port },
                points: Vec::new(),
            });
        }
    }
    // Scopes that log to the workspace get a To Workspace block fed by the same signals.
    let scopes: Vec<(String, String, String, usize, f64, f64)> = blocks
        .iter()
        .filter_map(|b| {
            let var = b.params.get("_log_var")?.clone();
            let fmt = b.params.get("_log_format").cloned().unwrap_or_default();
            let n = b
                .params
                .get("inputs")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            Some((b.id.clone(), var, fmt, n, b.x, b.y + b.h + 30.0))
        })
        .collect();
    for (sid, var, fmt, n, x, y) in scopes {
        if !fmt.starts_with("Structure") && fmt != "Array" {
            warnings.push(format!(
                "{}: scope data is saved as a structure with time instead of '{}'.",
                path, fmt
            ));
        }
        let tw = format!("{}_log", sid);
        let mut params = BTreeMap::new();
        params.insert("variable".to_string(), var.clone());
        params.insert(
            "format".to_string(),
            if fmt == "Array" {
                "Array"
            } else {
                "Structure With Time"
            }
            .to_string(),
        );
        blocks.push(BlockSpec {
            id: tw.clone(),
            kind: "ToWorkspace".into(),
            name: format!("{} (scope log)", var),
            x,
            y,
            w: 90.0,
            h: 34.0,
            params,
            system: None,
        });
        let feeds: Vec<(usize, PortRef)> = lines
            .iter()
            .filter(|l| l.dst.block == sid)
            .map(|l| (l.dst.port, l.src.clone()))
            .collect();
        if n <= 1 {
            if let Some((_, s)) = feeds.into_iter().next() {
                lines.push(Line {
                    src: s,
                    dst: PortRef { block: tw, port: 1 },
                    points: Vec::new(),
                });
            }
        } else {
            let mux = format!("{}_mux", sid);
            let mut mp = BTreeMap::new();
            mp.insert("inputs".to_string(), n.to_string());
            blocks.push(BlockSpec {
                id: mux.clone(),
                kind: "Mux".into(),
                name: format!("{} mux", var),
                x: x - 40.0,
                y,
                w: 10.0,
                h: 60.0,
                params: mp,
                system: None,
            });
            for (p, s) in feeds {
                lines.push(Line {
                    src: s,
                    dst: PortRef {
                        block: mux.clone(),
                        port: p,
                    },
                    points: Vec::new(),
                });
            }
            lines.push(Line {
                src: PortRef {
                    block: mux,
                    port: 1,
                },
                dst: PortRef { block: tw, port: 1 },
                points: Vec::new(),
            });
        }
    }
    // Bus elements are named after the signals that feed the Bus Creator.
    for (id, _) in bus_creators {
        if let Some(b) = blocks.iter_mut().find(|b| b.id == id) {
            let n: usize = b
                .params
                .get("names")
                .map(|s| s.split(',').count())
                .unwrap_or(2);
            let names: Vec<String> = (1..=n)
                .map(|k| {
                    bus_names
                        .get(&(id.clone(), k))
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("signal{}", k))
                })
                .collect();
            b.params.insert("names".into(), names.join(", "));
        }
    }
    (blocks, lines)
}

/// Wrap a parameter expression so it can be combined safely.
fn paren(s: &str) -> String {
    format!("({})", s.trim())
}

#[allow(clippy::too_many_arguments)]
fn convert_block(
    src: &Source,
    b: &Node,
    bt: &str,
    full: &str,
    n_in: usize,
    n_out: usize,
    warnings: &mut Vec<String>,
) -> Converted {
    let g = |k: &str| param(src, b, k);
    let gd = |k: &str, d: &str| param(src, b, k).unwrap_or_else(|| d.to_string());
    let mut ps: BTreeMap<String, String> = BTreeMap::new();
    let mut set = |k: &str, v: String| {
        ps.insert(k.to_string(), v);
    };
    let ts = |v: String, warnings: &mut Vec<String>| -> String {
        let t = v.trim();
        if t == "-1" || t.is_empty() {
            warnings.push(format!(
                "{}: inherited sample time (-1) was replaced by 1 s; set it in the block's dialog.",
                full
            ));
            "1".into()
        } else {
            t.to_string()
        }
    };
    let kind: &str = match bt {
        "Inport" => {
            set("port", gd("Port", "1"));
            "Inport"
        }
        "Outport" => {
            set("port", gd("Port", "1"));
            "Outport"
        }
        "Constant" => {
            set("value", gd("Value", "1"));
            "Constant"
        }
        "Ground" => {
            set("value", "0".into());
            "Constant"
        }
        "Gain" => {
            set("gain", gd("Gain", "1"));
            "Gain"
        }
        "Bias" => {
            set("bias", gd("Bias", "0"));
            "Bias"
        }
        "Abs" => "Abs",
        "Signum" => "Sign",
        "Clock" => "Clock",
        "Derivative" => "Derivative",
        "DotProduct" => "DotProduct",
        "Display" => "Display",
        "Terminator" => "Terminator",
        "Stop" => "StopSimulation",
        "Sum" => {
            let raw = gd("Inputs", "|++").replace('|', "");
            let signs = match raw.trim().parse::<usize>() {
                Ok(n) => "+".repeat(n.max(1)),
                Err(_) => raw.chars().filter(|c| *c == '+' || *c == '-').collect(),
            };
            set("signs", if signs.is_empty() { "++".into() } else { signs });
            "Sum"
        }
        "Product" => {
            let raw = gd("Inputs", "2");
            let ops = match raw.trim().parse::<usize>() {
                Ok(n) => "*".repeat(n.max(1)),
                Err(_) => raw.chars().filter(|c| *c == '*' || *c == '/').collect(),
            };
            set("ops", ops);
            "Product"
        }
        "Math" => {
            let op = gd("Operator", "exp");
            let f = match op.as_str() {
                "exp" | "log" | "log10" | "square" | "sqrt" | "reciprocal" => op.clone(),
                "10^u" => "pow10".into(),
                "magnitude^2" => "square".into(),
                _ => String::new(),
            };
            if f.is_empty() {
                return placeholder(full, bt, n_in, n_out, warnings);
            }
            set("function", f);
            "MathFunction"
        }
        "Sqrt" => {
            set("function", "sqrt".into());
            "MathFunction"
        }
        "Trigonometry" => {
            let op = gd("Operator", "sin");
            if ![
                "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
            ]
            .contains(&op.as_str())
            {
                return placeholder(full, bt, n_in, n_out, warnings);
            }
            set("function", op);
            "TrigFunction"
        }
        "MinMax" => {
            set("function", gd("Function", "min"));
            set("inputs", n_in.max(1).to_string());
            "MinMax"
        }
        "Fcn" => {
            set("expression", gd("Expr", gd("Expression", "u").as_str()));
            "Fcn"
        }
        "MATLABFcn" => {
            set("expression", gd("MATLABFcn", "sin(u)"));
            "Fcn"
        }
        "Integrator" => {
            set("initial", gd("InitialCondition", "0"));
            if g("LimitOutput").as_deref() == Some("on") {
                set("upper", gd("UpperSaturationLimit", "inf"));
                set("lower", gd("LowerSaturationLimit", "-inf"));
            }
            let reset = gd("ExternalReset", "none");
            if reset != "none" {
                set(
                    "reset",
                    match reset.as_str() {
                        "rising" | "falling" | "either" => reset.clone(),
                        _ => "either".into(),
                    },
                );
            }
            if gd("InitialConditionSource", "internal") == "external" {
                set("ic_source", "external".into());
            }
            "Integrator"
        }
        "TransferFcn" => {
            set("num", gd("Numerator", "[1]"));
            set("den", gd("Denominator", "[1 1]"));
            "TransferFcn"
        }
        "ZeroPole" => {
            set("zeros", gd("Zeros", "[1]"));
            set("poles", gd("Poles", "[0 -1]"));
            set("gain", gd("Gain", "[1]"));
            "ZeroPole"
        }
        "StateSpace" => {
            for (a, o) in [("A", "A"), ("B", "B"), ("C", "C"), ("D", "D"), ("X0", "x0")] {
                set(o, gd(a, "1"));
            }
            set("x0", gd("X0", "0"));
            "StateSpace"
        }
        "TransportDelay" => {
            set("delay", gd("DelayTime", "1"));
            set("initial", gd("InitialOutput", "0"));
            "TransportDelay"
        }
        "Step" => {
            set("time", gd("Time", "1"));
            set("initial", gd("Before", "0"));
            set("final", gd("After", "1"));
            "Step"
        }
        "Ramp" => {
            set("slope", gd("Slope", "1"));
            set("start", gd("Start", "0"));
            set("initial", gd("InitialOutput", "0"));
            "Ramp"
        }
        "Sin" => {
            set("amplitude", gd("Amplitude", "1"));
            set("bias", gd("Bias", "0"));
            set("frequency", gd("Frequency", "1"));
            set("phase", gd("Phase", "0"));
            if gd("SineType", "Time based") != "Time based" {
                warnings.push(format!(
                    "{}: sample-based sine converted to time-based.",
                    full
                ));
            }
            "SineWave"
        }
        "SignalGenerator" => {
            let wave = gd("WaveForm", "sine");
            let wave = match wave.as_str() {
                "sine" | "square" | "sawtooth" => wave,
                _ => {
                    warnings.push(format!("{}: wave form '{}' converted to sine.", full, wave));
                    "sine".into()
                }
            };
            set("wave", wave);
            set("amplitude", gd("Amplitude", "1"));
            let f = gd("Frequency", "1");
            set(
                "frequency",
                if gd("Units", "rad/sec").starts_with("rad") {
                    format!("{}/(2*pi)", paren(&f))
                } else {
                    f
                },
            );
            "SignalGenerator"
        }
        "DiscretePulseGenerator" => {
            set("amplitude", gd("Amplitude", "1"));
            let (period, width, delay) = (
                gd("Period", "2"),
                gd("PulseWidth", "1"),
                gd("PhaseDelay", "0"),
            );
            if gd("PulseType", "Sample based").starts_with("Sample") {
                let t = gd("SampleTime", "1");
                set("period", format!("{}*{}", paren(&period), paren(&t)));
                set("width", format!("100*{}/{}", paren(&width), paren(&period)));
                set("delay", format!("{}*{}", paren(&delay), paren(&t)));
            } else {
                set("period", period);
                set("width", width);
                set("delay", delay);
            }
            "PulseGenerator"
        }
        "UniformRandomNumber" | "RandomNumber" => {
            if bt == "UniformRandomNumber" {
                warnings.push(format!(
                    "{}: uniform noise replaced by Gaussian noise.",
                    full
                ));
            }
            set("mean", gd("Mean", "0"));
            set("variance", gd("Variance", "1"));
            set("seed", gd("Seed", "0"));
            "RandomNumber"
        }
        "FromWorkspace" => {
            set("data", gd("VariableName", "simin"));
            "FromWorkspace"
        }
        "ToWorkspace" => {
            set("variable", gd("VariableName", "simout"));
            if gd("SaveFormat", "Timeseries").starts_with("Structure") {
                set("format", "Structure With Time".into());
            }
            "ToWorkspace"
        }
        "Scope" | "Record" => {
            if bt == "Record" {
                return terminator_group(full, n_in.max(1));
            }
            let n = g("NumInputPorts")
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(n_in.max(1));
            set("inputs", n.to_string());
            // Logging to the workspace: older files use SaveToWorkspace/SaveName, newer
            // ones keep it in the scope's specification string.
            let spec = gd("ScopeSpecificationString", "").replace("&apos;", "'");
            let spec_flag = |k: &str| spec.contains(&format!("'{}',true", k));
            let spec_text = |k: &str| -> Option<String> {
                let key = format!("'{}','", k);
                let i = spec.find(&key)? + key.len();
                Some(spec[i..].split('\'').next()?.to_string())
            };
            let logging = gd("SaveToWorkspace", "off") == "on"
                || gd("DataLogging", "off") == "on"
                || spec_flag("DataLogging");
            if logging {
                let var = g("SaveName")
                    .or_else(|| g("DataLoggingVariableName"))
                    .or_else(|| spec_text("DataLoggingVariableName"))
                    .unwrap_or_else(|| "ScopeData".into());
                let format = g("DataFormat")
                    .or_else(|| g("DataLoggingSaveFormat"))
                    .or_else(|| spec_text("DataLoggingSaveFormat"))
                    .unwrap_or_else(|| "StructureWithTime".into());
                set("_log_var", var);
                set("_log_format", format);
            }
            "Scope"
        }
        "UnitDelay" => {
            set(
                "initial",
                g("InitialCondition")
                    .or_else(|| g("X0"))
                    .unwrap_or_else(|| "0".into()),
            );
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "UnitDelay"
        }
        "Delay" if gd("DelayLength", "2").trim() == "1" => {
            set("initial", gd("InitialCondition", "0"));
            set("ts", ts(gd("SampleTime", "-1"), warnings));
            "UnitDelay"
        }
        "DiscreteTransferFcn" => {
            set("num", gd("Numerator", "[1]"));
            set("den", gd("Denominator", "[1 0.5]"));
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "DiscreteTransferFcn"
        }
        "DiscreteFilter" => {
            // Coefficients in ascending powers of 1/z: pad the numerator to the
            // denominator's length and they read as descending powers of z.
            let (nu, de) = (
                paren(&gd("Numerator", "[1]")),
                paren(&gd("Denominator", "[1 0.5]")),
            );
            set(
                "num",
                format!("[{n}, zeros(1, numel({d}) - numel({n}))]", n = nu, d = de),
            );
            set(
                "den",
                format!("[{d}, zeros(1, numel({n}) - numel({d}))]", n = nu, d = de),
            );
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "DiscreteTransferFcn"
        }
        "DiscreteStateSpace" => {
            for k in ["A", "B", "C", "D"] {
                set(k, gd(k, "1"));
            }
            set("x0", gd("X0", "0"));
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "DiscreteStateSpace"
        }
        "DiscreteIntegrator" => {
            set("gain", gd("gainval", "1"));
            set("initial", gd("InitialCondition", "0"));
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "DiscreteIntegrator"
        }
        "ZeroOrderHold" => {
            set("ts", ts(gd("SampleTime", "1"), warnings));
            "ZeroOrderHold"
        }
        "Saturate" => {
            set("upper", gd("UpperLimit", "0.5"));
            set("lower", gd("LowerLimit", "-0.5"));
            "Saturation"
        }
        "DeadZone" => {
            set("start", gd("LowerValue", "-0.5"));
            set("end", gd("UpperValue", "0.5"));
            "DeadZone"
        }
        "Relay" => {
            set("on_point", gd("OnSwitchValue", "eps"));
            set("off_point", gd("OffSwitchValue", "eps"));
            set("on_value", gd("OnOutputValue", "1"));
            set("off_value", gd("OffOutputValue", "0"));
            "Relay"
        }
        "RateLimiter" => {
            set("rising", gd("RisingSlewLimit", "1"));
            set("falling", gd("FallingSlewLimit", "-1"));
            "RateLimiter"
        }
        "Quantizer" => {
            set("interval", gd("QuantizationInterval", "0.5"));
            "Quantizer"
        }
        "Lookup_n-D" | "Lookup" => {
            set(
                "breakpoints",
                g("BreakpointsForDimension1")
                    .or_else(|| g("InputValues"))
                    .unwrap_or_else(|| "[-5:5]".into()),
            );
            set(
                "table",
                g("Table")
                    .or_else(|| g("OutputValues"))
                    .unwrap_or_else(|| "tanh([-5:5])".into()),
            );
            "Lookup1D"
        }
        "Mux" | "Concatenate" => {
            set("inputs", n_in.max(1).to_string());
            "Mux"
        }
        "Demux" => {
            set("outputs", n_out.max(1).to_string());
            "Demux"
        }
        "Switch" => {
            let c = gd("Criteria", "u2 >= Threshold").replace("Threshold", "threshold");
            set("criteria", c);
            set("threshold", gd("Threshold", "0"));
            "Switch"
        }
        "MultiPortSwitch" => {
            if gd("DataPortOrder", "One-based contiguous").starts_with("Zero") {
                warnings.push(format!(
                    "{}: zero-based port order converted to one-based.",
                    full
                ));
            }
            set("inputs", n_in.saturating_sub(1).max(1).to_string());
            "MultiportSwitch"
        }
        "ManualSwitch" => return manual_switch(full, &gd("sw", "1")),
        // Rate transitions only matter for multi-rate code generation; pass the signal on.
        "RateTransition" | "SignalConversion" | "DataTypeConversion" | "SignalSpecification" => {
            set("gain", "1".into());
            "Gain"
        }
        "VariantSource" => {
            warnings.push(format!(
                "{}: variant conditions are not evaluated; the first input is used.",
                full
            ));
            return pick_first(full, n_in.max(1));
        }
        "RelationalOperator" => {
            set("op", gd("Operator", ">="));
            "RelationalOperator"
        }
        "Logic" => {
            set("op", gd("Operator", "AND"));
            set("inputs", n_in.max(1).to_string());
            "LogicalOperator"
        }
        "Goto" => {
            set("tag", gd("GotoTag", "A"));
            "Goto"
        }
        "From" => {
            set("tag", gd("GotoTag", "A"));
            "From"
        }
        "BusCreator" => {
            set(
                "names",
                (1..=n_in.max(1))
                    .map(|k| format!("signal{}", k))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            "BusCreator"
        }
        "BusSelector" => {
            set("select", gd("OutputSignals", "signal1").replace(['\n'], ""));
            "BusSelector"
        }
        "SubSystem" => return subsystem(src, b, full, n_in, n_out, warnings),
        // Control ports of conditional subsystems; the subsystem handles them.
        "EnablePort" => "EnablePort",
        "TriggerPort" => "TriggerPort",
        "ActionPort" => "ActionPort",
        "Reference" => {
            let lib = clean_name(&gd("SourceBlock", ""));
            match lib.as_str() {
                "slpidlib/PID Controller" | "slpidlib/PID Controller (2DOF)" => {
                    let ctype = gd("Controller", "PID");
                    set(
                        "P",
                        if ctype.contains('P') {
                            gd("P", "1")
                        } else {
                            "0".into()
                        },
                    );
                    set(
                        "I",
                        if ctype.contains('I') {
                            gd("I", "1")
                        } else {
                            "0".into()
                        },
                    );
                    set(
                        "D",
                        if ctype.contains('D') {
                            gd("D", "0")
                        } else {
                            "0".into()
                        },
                    );
                    if gd("TimeDomain", "Continuous-time").starts_with("Discrete") {
                        "DiscretePID"
                    } else {
                        set("N", gd("N", "100"));
                        "PID"
                    }
                }
                "simulink_extras/Transformations/Radians to Degrees" => {
                    set("gain", "180/pi".into());
                    "Gain"
                }
                "simulink_extras/Transformations/Degrees to Radians" => {
                    set("gain", "pi/180".into());
                    "Gain"
                }
                "simulink/Sources/Band-Limited White Noise" => {
                    warnings.push(format!(
                        "{}: band-limited white noise replaced by Gaussian noise.",
                        full
                    ));
                    set("variance", gd("Cov", "0.1"));
                    "RandomNumber"
                }
                _ => {
                    return placeholder(
                        full,
                        &format!("library block '{}'", lib),
                        n_in,
                        n_out,
                        warnings,
                    )
                }
            }
        }
        _ => return placeholder(full, bt, n_in, n_out, warnings),
    };
    Converted {
        kind: kind.to_string(),
        params: ps,
        system: None,
    }
}

fn subsystem(
    src: &Source,
    b: &Node,
    full: &str,
    n_in: usize,
    n_out: usize,
    warnings: &mut Vec<String>,
) -> Converted {
    // Signal Builder keeps its signals in binary data parts that are not read yet.
    if param(src, b, "Tag").as_deref() == Some("STV Subsys") {
        return placeholder(full, "the Signal Builder block", n_in, n_out, warnings);
    }
    let sys = b.kid("System").and_then(|s| match s.get("Ref") {
        Some(r) => src.systems.get(r).cloned(),
        None => Some(s.clone()),
    });
    let Some(sys) = sys else {
        return placeholder(full, "SubSystem without contents", n_in, n_out, warnings);
    };
    let has = |t: &str| sys.kids("Block").any(|k| k.get("BlockType") == Some(t));
    if has("TriggerPort") || has("ActionPort") {
        warnings.push(format!(
            "{}: triggered and if-action subsystems run as ordinary subsystems.",
            full
        ));
    }
    let enabled = has("EnablePort");
    let (blocks, lines) = convert_system(src, &sys, full, warnings);
    let mut params = BTreeMap::new();
    // Mask variables: new-style <Mask><MaskParameter Name=..><Value>..</Value>, or
    // old-style "MaskNames"/"MaskValueString" lists separated by '|'.
    if let Some(mask) = b.kid("Mask") {
        for mp in mask.kids("MaskParameter") {
            if let Some(n) = mp.get("Name") {
                let v = mp.kid("Value").map(|v| v.text.clone()).unwrap_or_default();
                params.insert(format!("mask:{}", n), v);
            }
        }
    }
    if let (Some(names), Some(values)) =
        (param(src, b, "MaskNames"), param(src, b, "MaskValueString"))
    {
        for (n, v) in names.split('|').zip(values.split('|')) {
            params.insert(format!("mask:{}", n.trim()), v.to_string());
        }
    }
    let blocks: Vec<BlockSpec> = blocks
        .into_iter()
        .filter(|b| !matches!(b.kind.as_str(), "EnablePort" | "TriggerPort" | "ActionPort"))
        .collect();
    let ids: std::collections::HashSet<String> = blocks.iter().map(|b| b.id.clone()).collect();
    let lines = lines
        .into_iter()
        .filter(|l| ids.contains(&l.src.block) && ids.contains(&l.dst.block))
        .collect();
    Converted {
        kind: if enabled {
            "EnabledSubsystem"
        } else {
            "Subsystem"
        }
        .into(),
        params,
        system: Some(Model {
            version: 1,
            name: full.rsplit('/').next().unwrap_or("").to_string(),
            solver: SolverSettings::default(),
            blocks,
            lines,
        }),
    }
}

fn port_block(kind: &str, k: usize, y: f64) -> BlockSpec {
    let mut params = BTreeMap::new();
    params.insert("port".to_string(), k.to_string());
    BlockSpec {
        id: format!("{}{}", kind.to_lowercase(), k),
        kind: kind.into(),
        name: format!("{}{}", if kind == "Inport" { "In" } else { "Out" }, k),
        x: if kind == "Inport" { 40.0 } else { 360.0 },
        y,
        w: 40.0,
        h: 26.0,
        params,
        system: None,
    }
}

fn simple(id: &str, kind: &str, x: f64, y: f64, params: &[(&str, &str)]) -> BlockSpec {
    BlockSpec {
        id: id.into(),
        kind: kind.into(),
        name: format!("{} {}", kind, id),
        x,
        y,
        w: 50.0,
        h: 40.0,
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        system: None,
    }
}

fn line(src: &str, sp: usize, dst: &str, dp: usize) -> Line {
    Line {
        src: PortRef {
            block: src.into(),
            port: sp,
        },
        dst: PortRef {
            block: dst.into(),
            port: dp,
        },
        points: Vec::new(),
    }
}

/// A subsystem with the original's port counts whose outputs are zero.
fn placeholder(
    full: &str,
    what: &str,
    n_in: usize,
    n_out: usize,
    warnings: &mut Vec<String>,
) -> Converted {
    warnings.push(format!(
        "{}: {} is not supported yet; it was replaced by a placeholder that outputs 0.",
        full, what
    ));
    let mut blocks = Vec::new();
    let mut lines = Vec::new();
    for k in 1..=n_in {
        let y = 40.0 + 60.0 * k as f64;
        blocks.push(port_block("Inport", k, y));
        let t = format!("t{}", k);
        blocks.push(simple(&t, "Terminator", 160.0, y, &[]));
        lines.push(line(&format!("inport{}", k), 1, &t, 1));
    }
    for k in 1..=n_out {
        let y = 40.0 + 60.0 * k as f64;
        blocks.push(port_block("Outport", k, y));
        let c = format!("z{}", k);
        blocks.push(simple(&c, "Constant", 240.0, y, &[("value", "0")]));
        lines.push(line(&c, 1, &format!("outport{}", k), 1));
    }
    Converted {
        kind: "Subsystem".into(),
        params: BTreeMap::new(),
        system: Some(Model {
            version: 1,
            name: String::new(),
            solver: SolverSettings::default(),
            blocks,
            lines,
        }),
    }
}

/// Sinks with several inputs (dashboard recorders): one terminator per input.
fn terminator_group(full: &str, n_in: usize) -> Converted {
    let mut w = Vec::new();
    let mut c = placeholder(full, "", n_in, 0, &mut w);
    if let Some(m) = c.system.as_mut() {
        m.name = full.rsplit('/').next().unwrap_or("").to_string();
    }
    c
}

/// Pass input 1 through and terminate the others (variant source).
fn pick_first(full: &str, n_in: usize) -> Converted {
    let mut blocks = vec![port_block("Outport", 1, 60.0)];
    let mut lines = Vec::new();
    for k in 1..=n_in {
        blocks.push(port_block("Inport", k, 20.0 + 60.0 * k as f64));
        if k == 1 {
            lines.push(line("inport1", 1, "outport1", 1));
        } else {
            let t = format!("t{}", k);
            blocks.push(simple(&t, "Terminator", 200.0, 20.0 + 60.0 * k as f64, &[]));
            lines.push(line(&format!("inport{}", k), 1, &t, 1));
        }
    }
    Converted {
        kind: "Subsystem".into(),
        params: BTreeMap::new(),
        system: Some(Model {
            version: 1,
            name: full.rsplit('/').next().unwrap_or("").to_string(),
            solver: SolverSettings::default(),
            blocks,
            lines,
        }),
    }
}

/// A two-input manual switch, fixed in the position it was saved in.
fn manual_switch(full: &str, sw: &str) -> Converted {
    let pick = if sw.trim() == "0" { 2 } else { 1 };
    let other = 3 - pick;
    let blocks = vec![
        port_block("Inport", 1, 60.0),
        port_block("Inport", 2, 140.0),
        port_block("Outport", 1, 60.0),
        simple("t", "Terminator", 200.0, 140.0, &[]),
    ];
    let lines = vec![
        line(&format!("inport{}", pick), 1, "outport1", 1),
        line(&format!("inport{}", other), 1, "t", 1),
    ];
    Converted {
        kind: "Subsystem".into(),
        params: BTreeMap::new(),
        system: Some(Model {
            version: 1,
            name: full.rsplit('/').next().unwrap_or("").to_string(),
            solver: SolverSettings::default(),
            blocks,
            lines,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdl_text_parses_sections_and_quoted_values() {
        let n = from_mdl(
            "Model {\n  Name \"m\"\n  System {\n    Block {\n      BlockType Gain\n      Name \"Gain\"\n      Gain \"2\"\n    }\n    Line {\n      SrcBlock \"Gain\"\n      SrcPort 1\n      Branch {\n        DstBlock \"Out\"\n        DstPort 1\n      }\n    }\n  }\n}\n",
        )
        .unwrap();
        let m = n.kid("Model").unwrap();
        let sys = m.kid("System").unwrap();
        assert_eq!(sys.kid("Block").unwrap().get("Gain"), Some("2"));
        let (s, d) = line_ends(sys.kid("Line").unwrap(), true);
        assert_eq!(s.unwrap().0, "Gain");
        assert_eq!(d[0].0, "Out");
    }

    #[test]
    fn port_refs_parse() {
        assert_eq!(port_ref("12#out:2"), Some(("12".into(), "out".into(), 2)));
        assert_eq!(port_ref("7#enable"), Some(("7".into(), "enable".into(), 1)));
    }
}
