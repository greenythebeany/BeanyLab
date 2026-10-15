//! `beany`            interactive REPL
//! `beany file.m`     run a script or function file
//! `beany -e "code"`  evaluate code
//! `beany serve [--port N] [--ui DIR]`  serve the desktop UI to a browser (development)
//! `beany conformance [DIR] [--bless] [--json FILE]`  run `.m` conformance tests

mod conformance;
mod serve;

use std::io::{BufRead, Write};

use beany_core::Session;

fn print_result(r: &beany_core::EvalResult) -> bool {
    let out = match r.output.rfind('\u{c}') {
        Some(i) => &r.output[i + 1..],
        None => &r.output,
    };
    print!("{}", out);
    if let Some(e) = &r.error {
        eprintln!("{}", e);
        return false;
    }
    true
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("serve") => serve::run(&args[1..]),
        Some("conformance") => std::process::exit(conformance::run(&args[1..])),
        Some("-e") => {
            let mut s = Session::new();
            let code = args.get(1).cloned().unwrap_or_default();
            let ok = print_result(&s.eval(&code));
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some("-h") | Some("--help") => {
            println!("BeanyLab {}\n\nUsage:\n  beany                 start the REPL\n  beany FILE.m          run a file\n  beany -e CODE         evaluate code\n  beany serve [--port N] [--ui DIR]  serve the UI on http://127.0.0.1:N", env!("CARGO_PKG_VERSION"));
        }
        Some(file) => {
            let mut s = Session::new();
            let path = std::path::Path::new(file);
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                let _ = s.set_cwd(&dir.to_string_lossy());
            }
            let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let ok = print_result(&s.run_file(&abs.to_string_lossy()));
            std::process::exit(if ok { 0 } else { 1 });
        }
        None => repl(),
    }
}

fn repl() {
    println!(
        "BeanyLab {} — type 'exit' to quit.\n",
        env!("CARGO_PKG_VERSION")
    );
    let mut s = Session::new();
    let stdin = std::io::stdin();
    let mut buf = String::new();
    loop {
        print!("{}", if buf.is_empty() { ">> " } else { "   " });
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let t = line.trim();
        if buf.is_empty() && (t == "exit" || t == "quit") {
            break;
        }
        buf.push_str(&line);
        // Keep reading while a block is open.
        if needs_more(&buf) {
            continue;
        }
        let code = std::mem::take(&mut buf);
        print_result(&s.eval(&code));
    }
}

fn needs_more(code: &str) -> bool {
    let mut depth: i32 = 0;
    for line in code.lines() {
        let l = line.split('%').next().unwrap_or("");
        for w in l.split(|c: char| !c.is_alphanumeric() && c != '_') {
            match w {
                "if" | "for" | "while" | "switch" | "try" | "function" | "parfor" => depth += 1,
                "end" => depth -= 1,
                _ => {}
            }
        }
    }
    depth > 0 || code.trim_end().ends_with("...")
}
