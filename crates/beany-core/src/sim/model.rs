//! Block-diagram model format (`.bmdl`): plain JSON so models diff well in version control.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub solver: SolverSettings,
    #[serde(default)]
    pub blocks: Vec<BlockSpec>,
    #[serde(default)]
    pub lines: Vec<Line>,
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSpec {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_w")]
    pub w: f64,
    #[serde(default = "default_h")]
    pub h: f64,
    /// Parameter values as typed in the dialog; expressions are evaluated in the workspace.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    /// Contents of a Subsystem block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<Box<Model>>,
}

fn default_w() -> f64 {
    70.0
}
fn default_h() -> f64 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRef {
    pub block: String,
    /// 1-based port number.
    pub port: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub src: PortRef,
    pub dst: PortRef,
    /// Optional routing waypoints drawn by the editor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverSettings {
    /// "auto", "ode45", "ode23", "ode23s", "ode1", "ode2", "ode4", "discrete".
    #[serde(default = "default_solver")]
    pub solver: String,
    #[serde(default = "default_stop")]
    pub stop_time: String,
    #[serde(default = "default_auto")]
    pub fixed_step: String,
    #[serde(default = "default_auto")]
    pub max_step: String,
    #[serde(default = "default_rel")]
    pub rel_tol: String,
    #[serde(default = "default_abs")]
    pub abs_tol: String,
}

fn default_solver() -> String {
    "auto".into()
}
fn default_stop() -> String {
    "10".into()
}
fn default_auto() -> String {
    "auto".into()
}
fn default_rel() -> String {
    "1e-3".into()
}
fn default_abs() -> String {
    "1e-6".into()
}

impl Default for SolverSettings {
    fn default() -> Self {
        SolverSettings {
            solver: default_solver(),
            stop_time: default_stop(),
            fixed_step: default_auto(),
            max_step: default_auto(),
            rel_tol: default_rel(),
            abs_tol: default_abs(),
        }
    }
}

impl Model {
    pub fn parse(text: &str) -> Result<Model, String> {
        let m: Model =
            serde_json::from_str(text).map_err(|e| format!("Invalid model file: {}", e))?;
        if m.version > FORMAT_VERSION {
            return Err(format!(
                "This model was saved by a newer BeanyLab (format {}).",
                m.version
            ));
        }
        Ok(m)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}
