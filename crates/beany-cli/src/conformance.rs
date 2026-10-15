//! Conformance and corpus runner.
//!
//! `beany conformance <dir> [--bless] [--json report.json]`
//!
//! Every `*.m` file under `<dir>` runs in a fresh session with its own folder as the
//! current folder. If a sibling `*.out` file exists, the captured command-window output
//! (plus any error report) must match it exactly; otherwise the file passes when it runs
//! without an error. `--bless` writes the current output to `*.out` files, but only for
//! tests that have none yet, unless `--bless-all` is given. Function files are helpers,
//! not tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use beany_core::Session;
use serde::Serialize;

#[derive(Serialize)]
struct CaseResult {
    file: String,
    area: String,
    passed: bool,
    kind: &'static str,
    detail: Option<String>,
}

#[derive(Serialize)]
struct Report {
    total: usize,
    passed: usize,
    percent: f64,
    areas: BTreeMap<String, (usize, usize)>,
    cases: Vec<CaseResult>,
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            // Helper folders hold functions that tests call, not tests themselves.
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name != "private" && !name.starts_with('+') && !name.starts_with('@') {
                collect(&p, out);
            }
        } else if p.extension().is_some_and(|e| e == "m") && !is_function_file(&p) {
            out.push(p);
        }
    }
}

/// Tests are scripts; function files next to them are helpers that tests call.
fn is_function_file(p: &Path) -> bool {
    let src = std::fs::read_to_string(p).unwrap_or_default();
    let first = src
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('%'));
    first.is_some_and(|l| l.starts_with("function ") || l == "function")
}

/// Output exactly as the command window would show it (after the last clc).
fn run_case(path: &Path) -> String {
    let mut s = Session::new();
    if let Some(dir) = path.parent() {
        let _ = s.set_cwd(&dir.to_string_lossy());
    }
    // Deterministic random numbers for every test.
    s.interp.rng = rand_seed();
    let r = s.run_file(&path.to_string_lossy());
    let mut text = r.output;
    if let Some(i) = text.rfind('\u{c}') {
        text = text[i + 1..].to_string();
    }
    if let Some(e) = r.error {
        // Error frames name the test file and line; keep only the message part for stable comparisons.
        let msg = e.split("\n\nError in ").next().unwrap_or(&e).to_string();
        text.push_str(&format!("[error] {}\n", msg));
    }
    text.replace("\r\n", "\n")
}

fn rand_seed() -> beany_core::interp::Rng {
    beany_core::interp::seeded_rng(42)
}

pub fn run(args: &[String]) -> i32 {
    let mut dirs = Vec::new();
    let mut bless = false;
    let mut bless_all = false;
    let mut json: Option<PathBuf> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bless" => bless = true,
            "--bless-all" => {
                bless = true;
                bless_all = true;
            }
            "--quiet" => quiet = true,
            "--json" => {
                json = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            d => dirs.push(PathBuf::from(d)),
        }
        i += 1;
    }
    if dirs.is_empty() {
        dirs.push(PathBuf::from("tests/conformance"));
    }
    let mut files = Vec::new();
    for d in &dirs {
        collect(d, &mut files);
    }
    let root = dirs[0].clone();
    let mut cases = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let area = rel
            .split('/')
            .next()
            .filter(|a| !a.ends_with(".m"))
            .unwrap_or("(root)")
            .to_string();
        let expected_path = f.with_extension("out");
        let actual = run_case(f);
        let (passed, kind, detail) = if bless && (bless_all || !expected_path.is_file()) {
            std::fs::write(&expected_path, &actual).expect("cannot write .out file");
            (true, "blessed", None)
        } else if expected_path.is_file() {
            let expected = std::fs::read_to_string(&expected_path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            if expected == actual {
                (true, "output", None)
            } else {
                (false, "output", Some(first_difference(&expected, &actual)))
            }
        } else if let Some(line) = actual.lines().find(|l| l.starts_with("[error] ")) {
            (false, "runs", Some(line.to_string()))
        } else {
            (true, "runs", None)
        };
        if !quiet || !passed {
            println!(
                "{} {}{}",
                if passed { "PASS" } else { "FAIL" },
                rel,
                detail
                    .as_ref()
                    .map(|d| format!("\n     {}", d.replace('\n', "\n     ")))
                    .unwrap_or_default()
            );
        }
        cases.push(CaseResult {
            file: rel,
            area,
            passed,
            kind,
            detail,
        });
    }
    let mut areas: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for c in &cases {
        let e = areas.entry(c.area.clone()).or_default();
        e.1 += 1;
        if c.passed {
            e.0 += 1;
        }
    }
    let passed = cases.iter().filter(|c| c.passed).count();
    let total = cases.len();
    let percent = if total == 0 {
        100.0
    } else {
        100.0 * passed as f64 / total as f64
    };
    println!("\nArea                         Passed");
    for (a, (p, t)) in &areas {
        println!(
            "  {:<26} {:>4} / {:<4} {:>5.1}%",
            a,
            p,
            t,
            100.0 * *p as f64 / (*t).max(1) as f64
        );
    }
    println!(
        "  {:<26} {:>4} / {:<4} {:>5.1}%",
        "TOTAL", passed, total, percent
    );
    if let Some(j) = json {
        let report = Report {
            total,
            passed,
            percent,
            areas,
            cases,
        };
        std::fs::write(&j, serde_json::to_string_pretty(&report).unwrap())
            .expect("cannot write JSON report");
    }
    if passed == total {
        0
    } else {
        1
    }
}

fn first_difference(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    for k in 0..e.len().max(a.len()) {
        let x = e.get(k).copied().unwrap_or("<end of output>");
        let y = a.get(k).copied().unwrap_or("<end of output>");
        if x != y {
            return format!("line {}:\n  expected: {}\n  actual:   {}", k + 1, x, y);
        }
    }
    "outputs differ in trailing whitespace".into()
}
