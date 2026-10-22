//! Writing models as `.slx` packages (zip of XML parts) so that other block-diagram
//! tools can open them. Only the diagram and the solver settings are written; each
//! block's remaining settings take the reading tool's defaults.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;

use super::blocks::ports_for;
use super::model::{BlockSpec, Model};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A block as the other tool names it: type, parameters, and (for subsystems) contents.
struct Out {
    block_type: String,
    params: Vec<(String, String)>,
    /// Library link for blocks that only exist as library blocks there.
    source_block: Option<String>,
}

fn param<'a>(b: &'a BlockSpec, k: &str, d: &'a str) -> &'a str {
    b.params.get(k).map(String::as_str).unwrap_or(d)
}

fn map_block(b: &BlockSpec) -> Out {
    let p = |k: &str, d: &str| param(b, k, d).to_string();
    let mk = |t: &str, ps: Vec<(&str, String)>| Out {
        block_type: t.into(),
        params: ps.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        source_block: None,
    };
    match b.kind.as_str() {
        "Constant" => mk(
            "Constant",
            vec![("Value", p("value", "1")), ("SampleTime", "inf".into())],
        ),
        "Step" => mk(
            "Step",
            vec![
                ("Time", p("time", "1")),
                ("Before", p("initial", "0")),
                ("After", p("final", "1")),
                ("SampleTime", "0".into()),
            ],
        ),
        "Ramp" => mk(
            "Ramp",
            vec![
                ("Slope", p("slope", "1")),
                ("Start", p("start", "0")),
                ("InitialOutput", p("initial", "0")),
            ],
        ),
        "SineWave" => mk(
            "Sin",
            vec![
                ("SineType", "Time based".into()),
                ("SampleTime", "0".into()),
                ("Amplitude", p("amplitude", "1")),
                ("Bias", p("bias", "0")),
                ("Frequency", p("frequency", "1")),
                ("Phase", p("phase", "0")),
            ],
        ),
        "PulseGenerator" => mk(
            "DiscretePulseGenerator",
            vec![
                ("PulseType", "Time based".into()),
                ("Amplitude", p("amplitude", "1")),
                ("Period", p("period", "1")),
                ("PulseWidth", p("width", "50")),
                ("PhaseDelay", p("delay", "0")),
            ],
        ),
        "SignalGenerator" => mk(
            "SignalGenerator",
            vec![
                ("WaveForm", p("wave", "sine")),
                ("Amplitude", p("amplitude", "1")),
                ("Frequency", p("frequency", "1")),
                ("Units", "Hertz".into()),
            ],
        ),
        "Clock" => mk("Clock", vec![]),
        "FromWorkspace" => mk("FromWorkspace", vec![("VariableName", p("data", "simin"))]),
        "RandomNumber" => mk(
            "RandomNumber",
            vec![
                ("Mean", p("mean", "0")),
                ("Variance", p("variance", "1")),
                ("Seed", p("seed", "0")),
            ],
        ),
        "Scope" => mk("Scope", vec![("NumInputPorts", p("inputs", "1"))]),
        "ToWorkspace" => mk(
            "ToWorkspace",
            vec![
                ("VariableName", p("variable", "simout")),
                (
                    "SaveFormat",
                    if p("format", "Array") == "Structure With Time" {
                        "StructureWithTime".into()
                    } else {
                        "Array".into()
                    },
                ),
            ],
        ),
        "Display" => mk("Display", vec![]),
        "Inport" => mk("Inport", vec![("Port", p("port", "1"))]),
        "Outport" => mk("Outport", vec![("Port", p("port", "1"))]),
        "Terminator" => mk("Terminator", vec![]),
        "StopSimulation" => mk("Stop", vec![]),
        "Gain" => mk("Gain", vec![("Gain", p("gain", "1"))]),
        "Bias" => mk("Bias", vec![("Bias", p("bias", "0"))]),
        "Abs" => mk("Abs", vec![]),
        "Sign" => mk("Signum", vec![]),
        "MathFunction" => {
            let f = p("function", "exp");
            mk(
                "Math",
                vec![("Operator", if f == "pow10" { "10^u".into() } else { f })],
            )
        }
        "TrigFunction" => mk("Trigonometry", vec![("Operator", p("function", "sin"))]),
        "MinMax" => mk(
            "MinMax",
            vec![
                ("Function", p("function", "min")),
                ("Inputs", p("inputs", "2")),
            ],
        ),
        "DotProduct" => mk("DotProduct", vec![]),
        "Fcn" => mk("Fcn", vec![("Expr", p("expression", "u(1)"))]),
        "UserFunction" => mk(
            "MATLABFcn",
            vec![("MATLABFcn", format!("{}(u)", p("function", "myfun")))],
        ),
        "Sum" => mk(
            "Sum",
            vec![
                ("IconShape", "rectangular".into()),
                ("Inputs", p("signs", "+-")),
            ],
        ),
        "Product" => mk("Product", vec![("Inputs", p("ops", "**"))]),
        "Integrator" => {
            let mut ps = vec![("InitialCondition", p("initial", "0"))];
            let (up, lo) = (p("upper", "inf"), p("lower", "-inf"));
            if up != "inf" || lo != "-inf" {
                ps.push(("LimitOutput", "on".into()));
                ps.push(("UpperSaturationLimit", up));
                ps.push(("LowerSaturationLimit", lo));
            }
            ps.push(("ExternalReset", p("reset", "none")));
            ps.push(("InitialConditionSource", p("ic_source", "internal")));
            mk("Integrator", ps)
        }
        "Derivative" => mk("Derivative", vec![]),
        "TransferFcn" => mk(
            "TransferFcn",
            vec![
                ("Numerator", p("num", "[1]")),
                ("Denominator", p("den", "[1 1]")),
            ],
        ),
        "StateSpace" => mk(
            "StateSpace",
            vec![
                ("A", p("A", "-1")),
                ("B", p("B", "1")),
                ("C", p("C", "1")),
                ("D", p("D", "0")),
                ("X0", p("x0", "0")),
            ],
        ),
        "ZeroPole" => mk(
            "ZeroPole",
            vec![
                ("Zeros", p("zeros", "[]")),
                ("Poles", p("poles", "[-1]")),
                ("Gain", p("gain", "1")),
            ],
        ),
        "TransportDelay" => mk(
            "TransportDelay",
            vec![
                ("DelayTime", p("delay", "1")),
                ("InitialOutput", p("initial", "0")),
            ],
        ),
        // Parallel PID with derivative filter as one transfer function:
        // P + I/s + D*N*s/(s+N) = ((P+D*N)s^2 + (P*N+I)s + I*N) / (s^2 + N*s).
        "PID" => {
            let (kp, ki, kd, n) = (p("P", "1"), p("I", "1"), p("D", "0"), p("N", "100"));
            mk(
                "TransferFcn",
                vec![
                    (
                        "Numerator",
                        format!(
                            "[({p})+({d})*({n}), ({p})*({n})+({i}), ({i})*({n})]",
                            p = kp,
                            i = ki,
                            d = kd,
                            n = n
                        ),
                    ),
                    ("Denominator", format!("[1, {}, 0]", n)),
                ],
            )
        }
        "UnitDelay" => mk(
            "UnitDelay",
            vec![
                ("InitialCondition", p("initial", "0")),
                ("SampleTime", p("ts", "1")),
            ],
        ),
        "DiscreteTransferFcn" => mk(
            "DiscreteTransferFcn",
            vec![
                ("Numerator", p("num", "[1]")),
                ("Denominator", p("den", "[1 0.5]")),
                ("SampleTime", p("ts", "1")),
            ],
        ),
        "DiscreteStateSpace" => mk(
            "DiscreteStateSpace",
            vec![
                ("A", p("A", "0.5")),
                ("B", p("B", "1")),
                ("C", p("C", "1")),
                ("D", p("D", "0")),
                ("X0", p("x0", "0")),
                ("SampleTime", p("ts", "1")),
            ],
        ),
        "DiscreteIntegrator" => mk(
            "DiscreteIntegrator",
            vec![
                ("gainval", p("gain", "1")),
                ("InitialCondition", p("initial", "0")),
                ("SampleTime", p("ts", "1")),
            ],
        ),
        "ZeroOrderHold" => mk("ZeroOrderHold", vec![("SampleTime", p("ts", "1"))]),
        "Saturation" => mk(
            "Saturate",
            vec![
                ("UpperLimit", p("upper", "0.5")),
                ("LowerLimit", p("lower", "-0.5")),
            ],
        ),
        "DeadZone" => mk(
            "DeadZone",
            vec![
                ("LowerValue", p("start", "-0.5")),
                ("UpperValue", p("end", "0.5")),
            ],
        ),
        "Relay" => mk(
            "Relay",
            vec![
                ("OnSwitchValue", p("on_point", "0.1")),
                ("OffSwitchValue", p("off_point", "-0.1")),
                ("OnOutputValue", p("on_value", "1")),
                ("OffOutputValue", p("off_value", "0")),
            ],
        ),
        "RateLimiter" => mk(
            "RateLimiter",
            vec![
                ("RisingSlewLimit", p("rising", "1")),
                ("FallingSlewLimit", p("falling", "-1")),
            ],
        ),
        "Quantizer" => mk(
            "Quantizer",
            vec![("QuantizationInterval", p("interval", "0.5"))],
        ),
        "Lookup1D" => mk(
            "Lookup_n-D",
            vec![
                ("NumberOfTableDimensions", "1".into()),
                ("BreakpointsForDimension1", p("breakpoints", "[-5:5]")),
                ("Table", p("table", "tanh([-5:5])")),
            ],
        ),
        "Mux" => mk("Mux", vec![("Inputs", p("inputs", "2"))]),
        "Demux" => mk("Demux", vec![("Outputs", p("outputs", "2"))]),
        "Switch" => mk(
            "Switch",
            vec![
                (
                    "Criteria",
                    p("criteria", "u2 >= threshold").replace("threshold", "Threshold"),
                ),
                ("Threshold", p("threshold", "0")),
            ],
        ),
        "MultiportSwitch" => mk("MultiPortSwitch", vec![("Inputs", p("inputs", "2"))]),
        "Goto" => mk(
            "Goto",
            vec![
                ("GotoTag", p("tag", "A")),
                ("TagVisibility", "global".into()),
            ],
        ),
        "From" => mk("From", vec![("GotoTag", p("tag", "A"))]),
        "BusCreator" => {
            let n = p("names", "signal1, signal2").split(',').count();
            mk("BusCreator", vec![("Inputs", n.to_string())])
        }
        "BusSelector" => mk(
            "BusSelector",
            vec![("OutputSignals", p("select", "signal1"))],
        ),
        "RelationalOperator" => mk("RelationalOperator", vec![("Operator", p("op", "<="))]),
        "LogicalOperator" => mk(
            "Logic",
            vec![("Operator", p("op", "AND")), ("Inputs", p("inputs", "2"))],
        ),
        "DiscretePID" => Out {
            block_type: "Reference".into(),
            params: vec![
                ("TimeDomain".into(), "Discrete-time".into()),
                ("P".into(), p("P", "1")),
                ("I".into(), p("I", "1")),
                ("D".into(), p("D", "0")),
            ],
            source_block: Some("slpidlib/PID Controller".into()),
        },
        other => mk(other, vec![]),
    }
}

struct Writer {
    sid: u32,
    xml: String,
}

impl Writer {
    fn next_sid(&mut self) -> u32 {
        self.sid += 1;
        self.sid
    }

    fn system(&mut self, m: &Model, depth: usize, enable_port: bool) -> Result<(), String> {
        let ind = "  ".repeat(depth);
        let _ = writeln!(self.xml, "{}<System>", ind);
        let mut sids: BTreeMap<&str, u32> = BTreeMap::new();
        for b in &m.blocks {
            let sid = self.next_sid();
            sids.insert(&b.id, sid);
            self.block(b, sid, depth + 1)?;
        }
        if enable_port {
            let sid = self.next_sid();
            let _ = writeln!(
                self.xml,
                "{0}  <Block BlockType=\"EnablePort\" Name=\"Enable\" SID=\"{1}\">\n{0}    <P Name=\"Position\">[20, 10, 40, 30]</P>\n{0}  </Block>",
                ind, sid
            );
        }
        // One line per source port; several destinations become branches.
        let mut by_src: BTreeMap<(String, usize), Vec<(String, usize)>> = BTreeMap::new();
        for l in &m.lines {
            by_src
                .entry((l.src.block.clone(), l.src.port))
                .or_default()
                .push((l.dst.block.clone(), l.dst.port));
        }
        for ((sb, sp), dsts) in by_src {
            let Some(s) = sids.get(sb.as_str()) else {
                continue;
            };
            let ends: Vec<String> = dsts
                .iter()
                .filter_map(|(db, dp)| {
                    let d = sids.get(db.as_str())?;
                    let dst = m.blocks.iter().find(|b| &b.id == db)?;
                    // The last input of an enabled subsystem is its enable port.
                    if dst.kind == "EnabledSubsystem" && *dp == subsystem_inputs(dst) + 1 {
                        Some(format!("{}#enable", d))
                    } else {
                        Some(format!("{}#in:{}", d, dp))
                    }
                })
                .collect();
            if ends.is_empty() {
                continue;
            }
            let _ = writeln!(self.xml, "{}  <Line>", ind);
            let _ = writeln!(self.xml, "{}    <P Name=\"Src\">{}#out:{}</P>", ind, s, sp);
            if ends.len() == 1 {
                let _ = writeln!(self.xml, "{}    <P Name=\"Dst\">{}</P>", ind, ends[0]);
            } else {
                for e in ends {
                    let _ = writeln!(
                        self.xml,
                        "{0}    <Branch>\n{0}      <P Name=\"Dst\">{1}</P>\n{0}    </Branch>",
                        ind, e
                    );
                }
            }
            let _ = writeln!(self.xml, "{}  </Line>", ind);
        }
        let _ = writeln!(self.xml, "{}</System>", ind);
        Ok(())
    }

    fn block(&mut self, b: &BlockSpec, sid: u32, depth: usize) -> Result<(), String> {
        let ind = "  ".repeat(depth);
        let pos = format!(
            "[{}, {}, {}, {}]",
            (b.x / 2.0).round(),
            (b.y / 2.0).round(),
            ((b.x + b.w) / 2.0).round(),
            ((b.y + b.h) / 2.0).round()
        );
        let sub = super::is_subsystem(&b.kind);
        let (bt, params, source) = if sub {
            ("SubSystem".to_string(), Vec::new(), None)
        } else {
            let o = map_block(b);
            (o.block_type, o.params, o.source_block)
        };
        let ports = if sub {
            let inner = b.system.as_deref();
            let (i, o) = inner.map(super::subsystem_ports).unwrap_or((1, 1));
            (i, o)
        } else {
            ports_for(&b.kind, &b.params, None)
        };
        let _ = writeln!(
            self.xml,
            "{}<Block BlockType=\"{}\" Name=\"{}\" SID=\"{}\">",
            ind,
            esc(&bt),
            esc(&b.name),
            sid
        );
        let _ = writeln!(
            self.xml,
            "{}  <P Name=\"Ports\">[{}, {}]</P>",
            ind, ports.0, ports.1
        );
        let _ = writeln!(self.xml, "{}  <P Name=\"Position\">{}</P>", ind, pos);
        if let Some(o) = b.params.get("_orient") {
            let _ = writeln!(self.xml, "{}  <P Name=\"Orientation\">{}</P>", ind, esc(o));
        }
        if let Some(src) = source {
            let _ = writeln!(
                self.xml,
                "{}  <P Name=\"SourceBlock\">{}</P>",
                ind,
                esc(&src)
            );
            let _ = writeln!(self.xml, "{}  <InstanceData>", ind);
            for (k, v) in &params {
                let _ = writeln!(self.xml, "{}    <P Name=\"{}\">{}</P>", ind, esc(k), esc(v));
            }
            let _ = writeln!(self.xml, "{}  </InstanceData>", ind);
        } else {
            for (k, v) in &params {
                let _ = writeln!(self.xml, "{}  <P Name=\"{}\">{}</P>", ind, esc(k), esc(v));
            }
        }
        if sub {
            let masks: Vec<(&str, &String)> = b
                .params
                .iter()
                .filter_map(|(k, v)| k.strip_prefix("mask:").map(|n| (n, v)))
                .collect();
            if !masks.is_empty() {
                let _ = writeln!(self.xml, "{}  <Mask>", ind);
                for (n, v) in masks {
                    let _ = writeln!(
                        self.xml,
                        "{0}    <MaskParameter Name=\"{1}\" Type=\"edit\">\n{0}      <Prompt>{1}</Prompt>\n{0}      <Value>{2}</Value>\n{0}    </MaskParameter>",
                        ind,
                        esc(n),
                        esc(v)
                    );
                }
                let _ = writeln!(self.xml, "{}  </Mask>", ind);
            }
            let empty = Model {
                version: 1,
                name: String::new(),
                solver: Default::default(),
                blocks: vec![],
                lines: vec![],
            };
            let inner = b.system.as_deref().unwrap_or(&empty);
            self.system(inner, depth + 1, b.kind == "EnabledSubsystem")?;
        }
        let _ = writeln!(self.xml, "{}</Block>", ind);
        Ok(())
    }
}

fn subsystem_inputs(b: &BlockSpec) -> usize {
    b.system
        .as_deref()
        .map(super::subsystem_ports)
        .unwrap_or((1, 1))
        .0
}

fn solver_xml(m: &Model) -> String {
    let s = &m.solver;
    let (name, kind) = match s.solver.as_str() {
        "ode45" | "ode23" | "ode23s" => (s.solver.as_str(), "Variable-step"),
        "ode1" | "ode2" | "ode4" => (s.solver.as_str(), "Fixed-step"),
        "discrete" => ("VariableStepDiscrete", "Variable-step"),
        _ => ("VariableStepAuto", "Variable-step"),
    };
    let auto = |v: &str| {
        if v.trim().is_empty() {
            "auto".to_string()
        } else {
            esc(v.trim())
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<ConfigSet>
  <Object ClassName="Simulink.ConfigSet">
    <Array PropName="Components" Type="Handle" Dimension="1*1">
      <Object ObjectID="2" ClassName="Simulink.SolverCC">
        <P Name="StartTime">0.0</P>
        <P Name="StopTime">{stop}</P>
        <P Name="AbsTol">{abs}</P>
        <P Name="FixedStep">{fixed}</P>
        <P Name="MaxStep">{max}</P>
        <P Name="RelTol">{rel}</P>
        <P Name="SolverType">{kind}</P>
        <P Name="SolverName">{name}</P>
      </Object>
    </Array>
    <P Name="Name">Configuration</P>
  </Object>
</ConfigSet>
"#,
        stop = auto(&s.stop_time),
        abs = auto(&s.abs_tol),
        fixed = auto(&s.fixed_step),
        max = auto(&s.max_step),
        rel = auto(&s.rel_tol),
        kind = kind,
        name = name
    )
}

/// The model as an `.slx` package.
pub fn to_slx(m: &Model) -> Result<Vec<u8>, String> {
    let mut w = Writer {
        sid: 0,
        xml: String::new(),
    };
    w.system(m, 2, false)?;
    let system = w.xml;
    let blockdiagram = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModelInformation Version=\"1.0\">\n  <Model>\n    <P Name=\"SavedCharacterEncoding\">UTF-8</P>\n{}  </Model>\n</ModelInformation>\n",
        system.replacen("<System>\n", &format!("<System>\n      <P Name=\"SIDHighWatermark\">{}</P>\n", w.sid), 1)
    );
    const CT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default ContentType="application/vnd.openxmlformats-package.relationships+xml" Extension="rels"/><Default ContentType="application/vnd.mathworks.simulink.mdl+xml" Extension="xml"/><Override ContentType="application/vnd.openxmlformats-package.core-properties+xml" PartName="/metadata/coreProperties.xml"/><Override ContentType="application/vnd.mathworks.simulink.configSet+xml" PartName="/simulink/configSet0.xml"/><Override ContentType="application/vnd.mathworks.simulink.configSetInfo+xml" PartName="/simulink/configSetInfo.xml"/></Types>"#;
    const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="blockDiagram" Target="simulink/blockdiagram.xml" Type="http://schemas.mathworks.com/simulink/2010/relationships/blockDiagram"/><Relationship Id="configSetInfo" Target="simulink/configSetInfo.xml" Type="http://schemas.mathworks.com/simulink/2014/relationships/configSetInfo"/><Relationship Id="coreprops" Target="metadata/coreProperties.xml" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"/></Relationships>"#;
    const CS_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="configSet0" Target="configSet0.xml" Type="http://schemas.mathworks.com/simulink/2014/relationships/configSet"/></Relationships>"#;
    const CS_INFO: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ConfigSetInfo>\n  <ConfigSet PartName=\"/simulink/configSet0.xml\" Active=\"true\">Configuration</ConfigSet>\n</ConfigSetInfo>\n";
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:creator>BeanyLab</dc:creator><dc:title>{}</dc:title></cp:coreProperties>"#,
        esc(&m.name)
    );
    let parts: [(&str, String); 7] = [
        ("[Content_Types].xml", CT.into()),
        ("_rels/.rels", RELS.into()),
        ("metadata/coreProperties.xml", core),
        ("simulink/blockdiagram.xml", blockdiagram),
        ("simulink/configSetInfo.xml", CS_INFO.into()),
        ("simulink/_rels/configSetInfo.xml.rels", CS_RELS.into()),
        ("simulink/configSet0.xml", solver_xml(m)),
    ];
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, text) in parts {
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}
