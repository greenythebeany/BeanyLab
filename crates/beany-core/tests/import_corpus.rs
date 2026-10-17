//! Opt-in check over a folder of .slx/.mdl models (not part of the repository):
//! `BEANY_MODEL_DIR=path cargo test --release --test import_corpus -- --ignored --nocapture`
//! Each model is imported and simulated; failures and import warnings are summarized.

use beany_core::sim::run::read_model_file;
use beany_core::Interp;

#[test]
#[ignore]
fn import_and_simulate_model_folder() {
    let Ok(dir) = std::env::var("BEANY_MODEL_DIR") else {
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("slx" | "mdl")))
        .collect();
    files.sort();
    let (mut imported, mut ran) = (0, 0);
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        match read_model_file(f) {
            Err(e) => println!("IMPORT FAIL {}: {}", name, e),
            Ok((m, warnings)) => {
                imported += 1;
                let mut it = Interp::new();
                it.cwd = f.parent().unwrap().to_path_buf();
                // Cap long or infinite runs.
                let mut m = m;
                if count(&m) == 0 {
                    println!("EMPTY {}", name);
                }
                m.solver.stop_time = format!("min({}, 20)", m.solver.stop_time);
                let r = beany_core::sim::run::run(&mut it, &m, &name);
                match r {
                    Ok((v, _)) => {
                        ran += 1;
                        if let beany_core::Value::Struct(st) = &v {
                            for k in &st.keys {
                                if k == "tout" {
                                    continue;
                                }
                                let mut val = st.get(0, k).unwrap().clone();
                                if let beany_core::Value::Struct(inner) = &val {
                                    if let Some(beany_core::Value::Struct(sig)) =
                                        inner.get(0, "signals")
                                    {
                                        if let Some(v) = sig.get(0, "values") {
                                            val = v.clone();
                                        }
                                    }
                                }
                                if let Ok(a) = val.to_num() {
                                    if a.rows > 0 {
                                        let last: Vec<String> = (0..a.cols)
                                            .map(|c| {
                                                format!("{:.6e}", a.data[c * a.rows + a.rows - 1])
                                            })
                                            .collect();
                                        println!("VAL {} {} {}", name, k, last.join(" "));
                                    }
                                }
                            }
                        }
                        println!(
                            "OK   {} ({} blocks, {} warnings)",
                            name,
                            count(&m),
                            warnings.len()
                        );
                    }
                    Err(e) => println!(
                        "SIM  {}: {} ({} warnings)",
                        name,
                        e.0.message,
                        warnings.len()
                    ),
                }
                for w in warnings.iter().take(3) {
                    println!("       - {}", w);
                }
            }
        }
    }
    println!(
        "{} files, {} imported, {} simulated",
        files.len(),
        imported,
        ran
    );
}

fn count(m: &beany_core::sim::Model) -> usize {
    m.blocks
        .iter()
        .map(|b| 1 + b.system.as_deref().map(count).unwrap_or(0))
        .sum()
}
