//! Opt-in: parse every `.m` file under `BEANY_CODE_DIR` and list the failures.
//!
//! ```text
//! BEANY_CODE_DIR=path/to/code cargo test -p beany-core --test parse_corpus -- --ignored --nocapture
//! ```

use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "m") {
            out.push(p);
        }
    }
}

#[test]
#[ignore]
fn parse_code_corpus() {
    let Ok(dir) = std::env::var("BEANY_CODE_DIR") else {
        eprintln!("BEANY_CODE_DIR is not set");
        return;
    };
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let mut failed = 0;
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        if let Err(e) = beany_core::parser::parse_file(&f.to_string_lossy(), &src) {
            failed += 1;
            println!("{}: {}", f.display(), e.message);
        }
    }
    println!("parsed {}/{} files", files.len() - failed, files.len());
}
