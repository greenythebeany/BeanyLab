//! Output, errors, workspace commands, files and environment.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::SeedableRng;

use super::*;
use crate::display::{self, Format};
use crate::error::MError;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("disp", |it, a, _| {
        check_args(&a, 1, 1)?;
        let s = display::disp(&a[0], it.format);
        it.print(&s);
        none()
    });
    b.insert("display", |it, a, _| {
        check_args(&a, 1, 1)?;
        let s = display::display("ans", &a[0], it.format);
        it.print(&s);
        none()
    });
    b.insert("fprintf", fprintf);
    b.insert("error", error);
    b.insert("warning", warning);
    b.insert("lastwarn", |it, _, _| {
        one(Value::chars(&it.last_warning.clone()))
    });
    b.insert("MException", |_, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let id = a[0].text()?;
        let msg = format_printf(&a[1].text()?, &a[2..])?;
        one(crate::interp::exception_value(&MError::with_id(id, msg)))
    });
    b.insert("throw", rethrow);
    b.insert("rethrow", rethrow);
    b.insert("throwAsCaller", rethrow);
    b.insert("getReport", |_, a, _| {
        check_args(&a, 1, 3)?;
        let Value::Struct(s) = &a[0] else {
            return Err(merr!("Input must be an exception."));
        };
        one(s
            .get(0, "message")
            .cloned()
            .unwrap_or_else(|| Value::chars("")))
    });
    b.insert("assert", |_, a, _| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        if a[0].is_true()? {
            return none();
        }
        if a.len() == 1 {
            return Err(MError::with_id("Beany:user:assertion", "Assertion failed."));
        }
        let first = a[1].text()?;
        if a.len() > 2 && first.contains(':') && !first.contains(' ') {
            let msg = format_printf(&a[2].text()?, &a[3..])?;
            return Err(MError::with_id(first, msg));
        }
        Err(MError::with_id(
            "Beany:user:assertion",
            format_printf(&first, &a[2..])?,
        ))
    });
    b.insert("tic", |it, _, nargout| {
        it.tic = Instant::now();
        if nargout > 0 {
            return one(Value::num(it.tic.elapsed().as_nanos() as f64));
        }
        none()
    });
    b.insert("toc", |it, _, nargout| {
        let t = it.tic.elapsed().as_secs_f64();
        if nargout > 0 {
            return one(Value::num(t));
        }
        it.print(&format!("Elapsed time is {:.6} seconds.\n", t));
        none()
    });
    b.insert("now", |_, _, _| one(Value::num(datenum_now())));
    b.insert("clock", |_, _, _| {
        let (y, mo, d, h, mi, s) = civil_now();
        one(Value::row(vec![
            y as f64, mo as f64, d as f64, h as f64, mi as f64, s,
        ]))
    });
    b.insert("datestr", |_, a, _| {
        let dn = if a.is_empty() {
            datenum_now()
        } else {
            a[0].scalar()?
        };
        one(Value::chars(&datestr(dn)))
    });
    b.insert("date", |_, _, _| {
        let s = datestr(datenum_now());
        one(Value::chars(&s[..11]))
    });
    b.insert("cputime", |it, _, _| {
        one(Value::num(it.tic.elapsed().as_secs_f64()))
    });
    b.insert("pause", |it, a, _| {
        it.flush_graphics();
        // `pause` alone or `pause('on'/'off')` doesn't wait here.
        let secs = if a.is_empty() || a[0].is_text() {
            0.0
        } else {
            a[0].scalar()?
        };
        if secs > 0.0 && secs.is_finite() {
            // Sleep in slices so the Stop button stays responsive.
            let end = Instant::now() + Duration::from_secs_f64(secs);
            while Instant::now() < end {
                if it.interrupt.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(
                    Duration::from_millis(10).min(end.saturating_duration_since(Instant::now())),
                );
            }
        }
        none()
    });
    b.insert("drawnow", |it, _, _| {
        it.flush_graphics();
        none()
    });
    b.insert("beep", |_, _, _| none());
    b.insert("input", |_, _, _| {
        Err(merr!(
            "input() is not supported yet in BeanyLab. Pass values as function arguments instead."
        ))
    });
    b.insert("keyboard", |_, _, _| {
        Err(merr!("keyboard is not supported yet."))
    });
    b.insert("exist", exist);
    b.insert("which", which);
    b.insert("who", who);
    b.insert("whos", whos);
    b.insert("clear", clear);
    b.insert("clearvars", clear);
    // A form feed tells the UI to clear the command window at this point in the output.
    b.insert("clc", |it, _, _| {
        it.print("\u{c}");
        none()
    });
    b.insert("home", |it, _, _| {
        it.print("\u{c}");
        none()
    });
    b.insert("format", |it, a, _| {
        let mode = if a.is_empty() {
            "short".to_string()
        } else {
            a[0].text()?.to_lowercase()
        };
        it.format = match mode.as_str() {
            "long" | "longg" => Format::Long,
            "short" | "shortg" | "compact" | "loose" => Format::Short,
            other => return Err(merr!("Unsupported format '{}'", other)),
        };
        none()
    });
    b.insert("pwd", |it, _, _| {
        one(Value::chars(&it.cwd.to_string_lossy()))
    });
    b.insert("cd", |it, a, nargout| {
        if a.is_empty() {
            if nargout > 0 {
                return one(Value::chars(&it.cwd.to_string_lossy()));
            }
            let s = format!("{}\n\n", it.cwd.display());
            it.print(&s);
            return none();
        }
        let target = crate::interp::clean_path(&it.cwd, &a[0].text()?);
        if !target.is_dir() {
            return Err(merr!(
                "Unable to change current folder to '{}' (Name is nonexistent or not a folder).",
                target.display()
            ));
        }
        it.cwd = target;
        none()
    });
    b.insert("ls", ls);
    b.insert("dir", dir);
    b.insert("what", |it, _, _| {
        let mut files: Vec<String> = std::fs::read_dir(&it.cwd)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| n.ends_with(".m"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let s = format!(
            "\nMATLAB-language code files in the current folder {}\n\n{}\n\n",
            it.cwd.display(),
            files.join("  ")
        );
        it.print(&s);
        none()
    });
    b.insert("mkdir", |it, a, _| {
        check_args(&a, 1, 2)?;
        let p = if a.len() == 2 {
            resolve(it, &a[0].text()?).join(a[1].text()?)
        } else {
            resolve(it, &a[0].text()?)
        };
        std::fs::create_dir_all(&p).map_err(|e| merr!("{}", e))?;
        one(Value::boolean(true))
    });
    b.insert("rmdir", |it, a, _| {
        check_args(&a, 1, 2)?;
        let p = resolve(it, &a[0].text()?);
        let r = if a.len() == 2 {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_dir(&p)
        };
        r.map_err(|e| merr!("{}", e))?;
        one(Value::boolean(true))
    });
    b.insert("delete", |it, a, _| {
        for v in &a {
            let p = resolve(it, &v.text()?);
            if let Err(e) = std::fs::remove_file(&p) {
                it.warn(&format!("File '{}' not found. ({})", p.display(), e));
            }
        }
        none()
    });
    b.insert("copyfile", |it, a, _| {
        check_args(&a, 2, 3)?;
        std::fs::copy(resolve(it, &a[0].text()?), resolve(it, &a[1].text()?))
            .map_err(|e| merr!("{}", e))?;
        one(Value::boolean(true))
    });
    b.insert("movefile", |it, a, _| {
        check_args(&a, 2, 3)?;
        std::fs::rename(resolve(it, &a[0].text()?), resolve(it, &a[1].text()?))
            .map_err(|e| merr!("{}", e))?;
        one(Value::boolean(true))
    });
    b.insert("isfile", |it, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(resolve(it, &a[0].text()?).is_file()))
    });
    b.insert("isfolder", |it, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(resolve(it, &a[0].text()?).is_dir()))
    });
    b.insert("isdir", |it, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(resolve(it, &a[0].text()?).is_dir()))
    });
    b.insert("fullfile", |_, a, _| {
        let mut p = PathBuf::new();
        for v in &a {
            let t = v.text()?;
            if !t.is_empty() {
                p.push(t);
            }
        }
        one(Value::chars(&p.to_string_lossy()))
    });
    b.insert("fileparts", |_, a, _| {
        check_args(&a, 1, 1)?;
        let t = a[0].text()?;
        let p = Path::new(&t);
        let dir = p
            .parent()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = p
            .file_stem()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = p
            .extension()
            .map(|d| format!(".{}", d.to_string_lossy()))
            .unwrap_or_default();
        Ok(vec![
            Value::chars(&dir),
            Value::chars(&name),
            Value::chars(&ext),
        ])
    });
    b.insert("filesep", |_, _, _| {
        one(Value::chars(std::path::MAIN_SEPARATOR_STR))
    });
    b.insert("pathsep", |_, _, _| {
        one(Value::chars(if cfg!(windows) { ";" } else { ":" }))
    });
    b.insert("tempdir", |_, _, _| {
        one(Value::chars(&std::env::temp_dir().to_string_lossy()))
    });
    b.insert("tempname", |_, _, _| {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        one(Value::chars(
            &std::env::temp_dir()
                .join(format!("tp{:x}", n))
                .to_string_lossy(),
        ))
    });
    b.insert("addpath", |it, a, _| {
        let mut end = false;
        let mut dirs = Vec::new();
        for v in &a {
            let t = v.text()?;
            if t == "-end" {
                end = true;
                continue;
            }
            if t == "-begin" {
                continue;
            }
            for part in t.split(if cfg!(windows) { ';' } else { ':' }) {
                if !part.is_empty() {
                    dirs.push(resolve(it, part));
                }
            }
        }
        for d in dirs.into_iter().rev() {
            it.path.retain(|p| *p != d);
            if end {
                it.path.push(d);
            } else {
                it.path.insert(0, d);
            }
        }
        none()
    });
    b.insert("rmpath", |it, a, _| {
        for v in &a {
            let d = resolve(it, &v.text()?);
            it.path.retain(|p| *p != d);
        }
        none()
    });
    b.insert("path", |it, _, nargout| {
        let s: Vec<String> = it
            .path
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if nargout > 0 {
            return one(Value::chars(&s.join(if cfg!(windows) { ";" } else { ":" })));
        }
        let text = format!(
            "\n\t\tSEARCH PATH\n\n{}\n\n",
            s.iter()
                .map(|p| format!("\t{}", p))
                .collect::<Vec<_>>()
                .join("\n")
        );
        it.print(&text);
        none()
    });
    b.insert("genpath", |it, a, _| {
        check_args(&a, 1, 1)?;
        let root = resolve(it, &a[0].text()?);
        let mut out = Vec::new();
        fn walk(p: &Path, out: &mut Vec<String>) {
            out.push(p.to_string_lossy().to_string());
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut dirs: Vec<PathBuf> = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                dirs.sort();
                for d in dirs {
                    let name = d
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if name.starts_with('@')
                        || name.starts_with('+')
                        || name == "private"
                        || name.starts_with('.')
                    {
                        continue;
                    }
                    walk(&d, out);
                }
            }
        }
        walk(&root, &mut out);
        one(Value::chars(&out.join(if cfg!(windows) {
            ";"
        } else {
            ":"
        })))
    });
    b.insert("fileread", |it, a, _| {
        check_args(&a, 1, 1)?;
        let p = resolve(it, &a[0].text()?);
        let s = std::fs::read_to_string(&p)
            .map_err(|_| merr!("Could not open file {}.", p.display()))?;
        one(Value::chars(&s))
    });
    b.insert("type", |it, a, _| {
        check_args(&a, 1, 1)?;
        let name = a[0].text()?;
        let p = locate_code(it, &name).ok_or_else(|| merr!("File '{}' not found.", name))?;
        let s = std::fs::read_to_string(&p).map_err(|e| merr!("{}", e))?;
        it.print(&format!("\n{}\n", s));
        none()
    });
    b.insert("help", help);
    b.insert("fopen", fopen);
    b.insert("fclose", |it, a, _| {
        check_args(&a, 1, 1)?;
        if a[0].is_text() && a[0].text()? == "all" {
            let ids: Vec<i32> = it.files.keys().copied().collect();
            for id in ids {
                close_file(it, id)?;
            }
            return one(Value::num(0.0));
        }
        let id = a[0].scalar()? as i32;
        close_file(it, id)?;
        one(Value::num(0.0))
    });
    b.insert("fgetl", |it, a, _| read_line(it, &a, false));
    b.insert("fgets", |it, a, _| read_line(it, &a, true));
    b.insert("feof", |it, a, _| {
        check_args(&a, 1, 1)?;
        let id = a[0].scalar()? as i32;
        let f = it
            .files
            .get(&id)
            .ok_or_else(|| merr!("Invalid file identifier."))?;
        one(bool_val(f.pos >= f.buf.len()))
    });
    b.insert("frewind", |it, a, _| {
        let id = a
            .first()
            .ok_or_else(|| merr!("Not enough input arguments."))?
            .scalar()? as i32;
        if let Some(f) = it.files.get_mut(&id) {
            f.pos = 0;
        }
        none()
    });
    b.insert("readmatrix", |it, a, _| read_numeric(it, &a));
    b.insert("csvread", |it, a, _| read_numeric(it, &a));
    b.insert("dlmread", |it, a, _| read_numeric(it, &a));
    b.insert("readlines", |it, a, _| {
        check_args(&a, 1, 1)?;
        let p = resolve(it, &a[0].text()?);
        let s = std::fs::read_to_string(&p)
            .map_err(|_| merr!("Unable to find or open '{}'.", p.display()))?;
        let lines: Vec<String> = s.lines().map(|l| l.to_string()).collect();
        let n = lines.len();
        one(Value::Str(Array::new(n, 1, lines)))
    });
    b.insert("writematrix", |it, a, _| write_numeric(it, &a, ","));
    b.insert("csvwrite", |it, a, _| {
        check_args(&a, 2, 2)?;
        write_numeric(it, &[a[1].clone(), a[0].clone()], ",")
    });
    b.insert("dlmwrite", |it, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let delim = if a.len() > 2 && a[2].is_text() {
            unescape_delim(&a[2].text()?)
        } else {
            ",".into()
        };
        write_numeric(it, &[a[1].clone(), a[0].clone()], &delim)
    });
    b.insert("save", save);
    b.insert("load", load);
    b.insert("eval", |it, a, nargout| {
        check_args(&a, 1, 2)?;
        let code = a[0].text()?;
        if nargout > 0 {
            let e = crate::parser::parse_expression(&code)?;
            return it.eval_multi(&e, nargout);
        }
        match it.eval_string(&code) {
            Ok(()) => none(),
            Err(e) if a.len() == 2 && !e.interrupt => {
                let catch = a[1].text()?;
                it.eval_string(&catch)?;
                none()
            }
            Err(e) => Err(e),
        }
    });
    b.insert("evalin", |it, a, _| {
        check_args(&a, 2, 2)?;
        let code = a[1].text()?;
        // Base and caller both run in the current frame's base for simplicity.
        let saved: Vec<crate::interp::Frame> = it.frames.drain(1..).collect();
        let r = it.eval_string(&code);
        it.frames.extend(saved);
        r?;
        none()
    });
    b.insert("assignin", |it, a, _| {
        check_args(&a, 3, 3)?;
        let ws = a[0].text()?;
        let name = a[1].text()?;
        if ws == "base" {
            it.frames[0].vars.insert(name, a[2].clone());
        } else {
            let n = it.frames.len();
            let idx = n.saturating_sub(2);
            it.frames[idx].vars.insert(name, a[2].clone());
        }
        none()
    });
    b.insert("run", |it, a, _| {
        check_args(&a, 1, 1)?;
        let name = a[0].text()?;
        let mut p = resolve(it, &name);
        if p.extension().is_none() {
            p.set_extension("m");
        }
        if !p.is_file() {
            return Err(merr!("{} not found.", name));
        }
        let saved = it.cwd.clone();
        let dir = p.parent().map(|d| d.to_path_buf());
        if let Some(d) = &dir {
            if d.is_dir() {
                it.cwd = d.clone();
            }
        }
        let r = it.run_file(&p);
        it.cwd = saved;
        r?;
        none()
    });
    b.insert("nargin", |it, a, _| {
        if !a.is_empty() {
            let name = a[0].text()?;
            let p =
                locate_code(it, &name).ok_or_else(|| merr!("Function '{}' not found.", name))?;
            let unit = it.load_unit(&p)?;
            let f = unit
                .functions
                .first()
                .ok_or_else(|| merr!("'{}' is a script.", name))?;
            let n = f.params.len() as f64;
            return one(Value::num(
                if f.params.last().is_some_and(|p| p == "varargin") {
                    -n
                } else {
                    n
                },
            ));
        }
        if it.frames.len() <= 1 {
            return Err(merr!(
                "You can only call nargin/nargout from within a function."
            ));
        }
        one(Value::num(it.frame().nargin as f64))
    });
    b.insert("nargout", |it, a, _| {
        if !a.is_empty() {
            let name = a[0].text()?;
            let p =
                locate_code(it, &name).ok_or_else(|| merr!("Function '{}' not found.", name))?;
            let unit = it.load_unit(&p)?;
            let f = unit
                .functions
                .first()
                .ok_or_else(|| merr!("'{}' is a script.", name))?;
            return one(Value::num(f.outputs.len() as f64));
        }
        if it.frames.len() <= 1 {
            return Err(merr!(
                "You can only call nargin/nargout from within a function."
            ));
        }
        one(Value::num(it.frame().nargout as f64))
    });
    b.insert("narginchk", |it, a, _| {
        check_args(&a, 2, 2)?;
        let n = it.frame().nargin as f64;
        if n < a[0].scalar()? {
            return Err(MError::with_id(
                "Beany:narginchk:notEnoughInputs",
                "Not enough input arguments.",
            ));
        }
        if n > a[1].scalar()? {
            return Err(MError::with_id(
                "Beany:narginchk:tooManyInputs",
                "Too many input arguments.",
            ));
        }
        none()
    });
    b.insert("nargoutchk", |_, _, _| none());
    b.insert("ispc", |_, _, _| one(bool_val(cfg!(windows))));
    b.insert("isunix", |_, _, _| one(bool_val(cfg!(unix))));
    b.insert("ismac", |_, _, _| one(bool_val(cfg!(target_os = "macos"))));
    b.insert("isdeployed", |_, _, _| one(bool_val(false)));
    b.insert("computer", |_, _, _| {
        one(Value::chars(if cfg!(windows) {
            "PCWIN64"
        } else if cfg!(target_os = "macos") {
            "MACA64"
        } else {
            "GLNXA64"
        }))
    });
    b.insert("version", |_, _, _| {
        one(Value::chars(concat!(
            "BeanyLab ",
            env!("CARGO_PKG_VERSION")
        )))
    });
    b.insert("ver", |it, _, _| {
        let s = format!("-----------------------------------------------------------------------\nBeanyLab Version {}\nOperating System: {}\n-----------------------------------------------------------------------\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
        it.print(&s);
        none()
    });
    b.insert("getenv", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::chars(
            &std::env::var(a[0].text()?).unwrap_or_default(),
        ))
    });
    b.insert("setenv", |_, a, _| {
        check_args(&a, 1, 2)?;
        let v = if a.len() > 1 {
            a[1].text()?
        } else {
            String::new()
        };
        std::env::set_var(a[0].text()?, v);
        none()
    });
    b.insert("system", system);
    b.insert("rng", |it, a, _| {
        let seed = match a.first() {
            None => 0,
            Some(v) if v.is_text() => match v.text()?.as_str() {
                "shuffle" => SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0),
                _ => 0,
            },
            Some(v) => v.scalar()? as u64,
        };
        it.rng = rand::rngs::StdRng::seed_from_u64(seed);
        none()
    });
    b.insert("exit", |it, _, _| {
        it.print("exit is ignored inside the BeanyLab desktop.\n");
        none()
    });
    b.insert("quit", |it, _, _| {
        it.print("quit is ignored inside the BeanyLab desktop.\n");
        none()
    });
    b.insert("commandwindow", |_, _, _| none());
    b.insert("more", |_, _, _| none());
    b.insert("diary", |_, _, _| none());
}

pub fn resolve(it: &Interp, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(rest) = p.strip_prefix("~/") {
        dirs_home().join(rest)
    } else {
        it.cwd.join(path)
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn locate_code(it: &Interp, name: &str) -> Option<PathBuf> {
    let n = name.trim_end_matches(".m");
    if let Some(p) = it.find_file(n, true) {
        return Some(p);
    }
    let p = resolve(it, name);
    if p.is_file() {
        return Some(p);
    }
    let p = resolve(it, &format!("{}.m", n));
    p.is_file().then_some(p)
}

fn fprintf(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    // fprintf(fid, fmt, ...) when the first argument is a numeric file id.
    if let Value::Num(m, _) = &a[0] {
        if m.numel() == 1 && a.len() >= 2 && a[1].is_text() {
            let fid = m.data[0] as i32;
            let s = format_printf(&a[1].text()?, &a[2..])?;
            match fid {
                1 => it.print(&s),
                2 => it.print(&s),
                _ => {
                    let f = it.files.get_mut(&fid).ok_or_else(|| merr!("Invalid file identifier. Use fopen to generate a valid file identifier."))?;
                    if !f.write {
                        return Err(merr!("File is not open for writing."));
                    }
                    f.buf.extend_from_slice(s.as_bytes());
                }
            }
            return none();
        }
    }
    let s = format_printf(&a[0].text()?, &a[1..])?;
    it.print(&s);
    none()
}

fn error(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return none();
    }
    if let Value::Struct(s) = &a[0] {
        let msg = s
            .get(0, "message")
            .and_then(|v| v.text().ok())
            .unwrap_or_default();
        let id = s
            .get(0, "identifier")
            .and_then(|v| v.text().ok())
            .unwrap_or_default();
        return Err(user_err(MError::with_id(id, msg)));
    }
    let first = a[0].text()?;
    let is_id = a.len() > 1
        && first.contains(':')
        && !first.contains(char::is_whitespace)
        && !first.ends_with(':');
    let (id, msg) = if is_id {
        (first, format_printf(&a[1].text()?, &a[2..])?)
    } else if a.len() > 1 {
        (String::new(), format_printf(&first, &a[1..])?)
    } else {
        (String::new(), first)
    };
    Err(user_err(MError::with_id(id, msg)))
}

/// User-raised errors carry their own text: an empty origin suppresses "Error using <builtin>".
fn user_err(mut e: MError) -> MError {
    e.origin = Some(String::new());
    e
}

fn rethrow(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    let Value::Struct(s) = &a[0] else {
        return Err(merr!("Input must be an MException."));
    };
    let msg = s
        .get(0, "message")
        .and_then(|v| v.text().ok())
        .unwrap_or_default();
    let id = s
        .get(0, "identifier")
        .and_then(|v| v.text().ok())
        .unwrap_or_default();
    Err(user_err(MError::with_id(id, msg)))
}

fn warning(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return none();
    }
    let first = a[0].text()?;
    match first.as_str() {
        "off" => {
            if a.len() == 1 || a[1].text()? == "all" {
                it.warnings_on = false;
            }
            return none();
        }
        "on" => {
            if a.len() == 1 || a[1].text()? == "all" {
                it.warnings_on = true;
            }
            return none();
        }
        "query" => return none(),
        _ => {}
    }
    let is_id = a.len() > 1 && first.contains(':') && !first.contains(char::is_whitespace);
    let msg = if is_id {
        format_printf(&a[1].text()?, &a[2..])?
    } else {
        format_printf(&first, &a[1..])?
    };
    it.warn(&msg);
    none()
}

fn exist(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let name = a[0].text()?;
    let kind = if a.len() > 1 {
        a[1].text()?
    } else {
        String::new()
    };
    let r = if (kind.is_empty() || kind == "var") && it.is_var(&name) {
        1.0
    } else if kind == "var" {
        0.0
    } else if (kind.is_empty() || kind == "file") && it.find_file(&name, true).is_some() {
        2.0
    } else if (kind.is_empty() || kind == "builtin") && it.builtins.contains_key(name.as_str()) {
        5.0
    } else if (kind.is_empty() || kind == "dir") && resolve(it, &name).is_dir() {
        7.0
    } else if (kind.is_empty() || kind == "file") && resolve(it, &name).is_file() {
        2.0
    } else {
        0.0
    };
    one(Value::num(r))
}

fn which(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let name = a[0].text()?;
    let s = if it.is_var(&name) {
        format!("{} is a variable.", name)
    } else if let Some(p) = it.find_file(&name, true) {
        p.to_string_lossy().to_string()
    } else if it.builtins.contains_key(name.as_str()) {
        format!("built-in ({})", name)
    } else {
        format!("'{}' not found.", name)
    };
    if nargout > 0 {
        return one(Value::chars(&s));
    }
    it.print(&format!("{}\n", s));
    none()
}

fn who(it: &mut Interp, _: Vec<Value>, nargout: usize) -> R {
    let names: Vec<String> = it.workspace().into_iter().map(|(n, _)| n).collect();
    if nargout > 0 {
        let n = names.len();
        return one(Value::Cell(Array::new(
            n,
            1,
            names.iter().map(|s| Value::chars(s)).collect(),
        )));
    }
    if names.is_empty() {
        return none();
    }
    let s = format!("\nYour variables are:\n\n{}\n\n", names.join("  "));
    it.print(&s);
    none()
}

pub fn bytes_of(v: &Value) -> usize {
    match v {
        Value::Num(a, c) => {
            let per = match c {
                NumClass::Double => 8,
                NumClass::Single => 4,
                NumClass::Logical => 1,
                NumClass::Int(k) => match k {
                    IntKind::I8 | IntKind::U8 => 1,
                    IntKind::I16 | IntKind::U16 => 2,
                    IntKind::I32 | IntKind::U32 => 4,
                    _ => 8,
                },
            };
            a.numel() * per
        }
        Value::Char(a) => a.numel() * 2,
        Value::Str(a) => a.data.iter().map(|s| 16 + s.len() * 2).sum::<usize>() + 132,
        Value::Cell(a) => a.data.iter().map(|x| 104 + bytes_of(x)).sum(),
        Value::Struct(s) => {
            s.arr
                .data
                .iter()
                .flat_map(|e| e.iter())
                .map(|x| 104 + bytes_of(x))
                .sum::<usize>()
                + 64 * s.keys.len()
        }
        Value::Func(_) => 32,
        Value::Obj(o) => o.props.arr.data[0].iter().map(bytes_of).sum::<usize>() + 64,
    }
}

fn whos(it: &mut Interp, _: Vec<Value>, _: usize) -> R {
    let vars = it.workspace();
    if vars.is_empty() {
        return none();
    }
    let mut s = format!(
        "  {:<12}{:>12}{:>16}  {:<12}\n\n",
        "Name", "Size", "Bytes", "Class"
    );
    for (n, v) in vars {
        let (r, c) = v.dims();
        s.push_str(&format!(
            "  {:<12}{:>12}{:>16}  {:<12}\n",
            n,
            format!("{}x{}", r, c),
            bytes_of(&v),
            v.class_name()
        ));
    }
    s.push('\n');
    it.print(&s);
    none()
}

fn clear(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        it.frame_mut().vars.clear();
        it.frame_mut().globals.clear();
        return none();
    }
    for v in &a {
        let name = v.text()?;
        match name.as_str() {
            "all" | "-all" => {
                it.frame_mut().vars.clear();
                it.frame_mut().globals.clear();
                it.globals.clear();
                it.persistents.clear();
                it.clear_cache();
            }
            "variables" | "-variables" | "vars" => {
                it.frame_mut().vars.clear();
            }
            "functions" | "-functions" => {
                it.persistents.clear();
                it.clear_cache();
            }
            "global" | "-global" => {
                it.globals.clear();
                it.frame_mut().globals.clear();
            }
            "classes" | "import" => {}
            pat if pat.contains('*') => {
                let re =
                    regex::Regex::new(&format!("^{}$", regex::escape(pat).replace("\\*", ".*")))
                        .unwrap();
                let names: Vec<String> = it
                    .frame()
                    .vars
                    .keys()
                    .filter(|k| re.is_match(k))
                    .cloned()
                    .collect();
                for n in names {
                    it.clear_var(&n);
                }
            }
            n => it.clear_var(n),
        }
    }
    none()
}

fn list_dir(
    it: &Interp,
    pattern: Option<String>,
) -> MResult<Vec<(String, bool, u64, Option<SystemTime>)>> {
    let (dir, glob) = match pattern {
        None => (it.cwd.clone(), None),
        Some(p) => {
            let full = resolve(it, &p);
            if full.is_dir() {
                (full, None)
            } else {
                let parent = full
                    .parent()
                    .map(|x| x.to_path_buf())
                    .unwrap_or_else(|| it.cwd.clone());
                let g = full.file_name().map(|x| x.to_string_lossy().to_string());
                (parent, g)
            }
        }
    };
    let re = glob.map(|g| {
        regex::Regex::new(&format!(
            "(?i)^{}$",
            regex::escape(&g).replace("\\*", ".*").replace("\\?", ".")
        ))
        .unwrap()
    });
    let mut out = Vec::new();
    let rd = std::fs::read_dir(&dir).map_err(|_| merr!("Folder '{}' not found.", dir.display()))?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(re) = &re {
            if !re.is_match(&name) {
                continue;
            }
        }
        let md = e.metadata().ok();
        out.push((
            name,
            md.as_ref().is_some_and(|m| m.is_dir()),
            md.as_ref().map_or(0, |m| m.len()),
            md.and_then(|m| m.modified().ok()),
        ));
    }
    out.sort_by_key(|a| a.0.to_lowercase());
    Ok(out)
}

fn ls(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let pat = a.first().map(|v| v.text()).transpose()?;
    let entries = list_dir(it, pat)?;
    let names: Vec<String> = entries.iter().map(|e| e.0.clone()).collect();
    if nargout > 0 {
        return one(Value::chars(&names.join("  ")));
    }
    let mut s = String::from("\n");
    s.push_str(&names.join("  "));
    s.push_str("\n\n");
    it.print(&s);
    none()
}

fn dir(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let pat = a.first().map(|v| v.text()).transpose()?;
    let mut entries = list_dir(it, pat)?;
    entries.insert(0, ("..".into(), true, 0, None));
    entries.insert(0, (".".into(), true, 0, None));
    if nargout == 0 {
        let names: Vec<String> = entries.iter().map(|e| e.0.clone()).collect();
        let s = format!("\n{}\n\n", names.join("  "));
        it.print(&s);
        return none();
    }
    let folder = it.cwd.to_string_lossy().to_string();
    let elems: Vec<Vec<Value>> = entries
        .iter()
        .map(|(n, d, b, t)| {
            let dn = t
                .map(|t| {
                    t.duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs_f64() / 86400.0 + 719529.0)
                        .unwrap_or(0.0)
                })
                .unwrap_or(0.0);
            vec![
                Value::chars(n),
                Value::chars(&folder),
                Value::chars(&if dn > 0.0 { datestr(dn) } else { String::new() }),
                Value::num(*b as f64),
                Value::boolean(*d),
                Value::num(dn),
            ]
        })
        .collect();
    let n = elems.len();
    one(Value::Struct(StructArr {
        keys: ["name", "folder", "date", "bytes", "isdir", "datenum"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        arr: Array::new(n, 1, elems),
    }))
}

fn help(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        it.print("Type 'help name' to see the help comments of a function file in the current folder or path.\n");
        return none();
    }
    let name = a[0].text()?;
    if let Some(p) = locate_code(it, &name) {
        let src = std::fs::read_to_string(&p).unwrap_or_default();
        let mut lines = Vec::new();
        let mut started = false;
        for l in src.lines() {
            let t = l.trim();
            if t.starts_with('%') {
                started = true;
                lines.push(format!(" {}", t.trim_start_matches('%')));
            } else if started {
                break;
            } else if t.is_empty() || t.starts_with("function") {
                continue;
            } else {
                break;
            }
        }
        let text = if lines.is_empty() {
            format!("No help found for {}.\n", name)
        } else {
            format!("{}\n", lines.join("\n"))
        };
        it.print(&text);
    } else if it.builtins.contains_key(name.as_str()) {
        it.print(&format!(" {} is a built-in function.\n", name));
    } else {
        it.print(&format!("{} not found.\n", name));
    }
    none()
}

fn fopen(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 4)?;
    let name = a[0].text()?;
    if name == "all" {
        let ids: Vec<f64> = it.files.keys().map(|k| *k as f64).collect();
        return one(Value::row(ids));
    }
    let mode = if a.len() > 1 {
        a[1].text()?
    } else {
        "r".into()
    };
    let p = resolve(it, &name);
    let write = mode.contains('w') || mode.contains('a');
    let mut buf = Vec::new();
    if !write || mode.contains('a') {
        match std::fs::read(&p) {
            Ok(b) => buf = b,
            Err(_) if !write => {
                return Ok(vec![
                    Value::num(-1.0),
                    Value::chars(&format!("Unable to open file '{}'.", p.display())),
                ])
            }
            Err(_) => {}
        }
    }
    let id = (3..).find(|k| !it.files.contains_key(k)).unwrap();
    it.files.insert(
        id,
        crate::interp::OpenFile {
            path: p,
            write,
            buf,
            pos: 0,
        },
    );
    Ok(vec![Value::num(id as f64), Value::chars("")])
}

fn close_file(it: &mut Interp, id: i32) -> MResult<()> {
    let f = it.files.remove(&id).ok_or_else(|| {
        merr!("Invalid file identifier. Use fopen to generate a valid file identifier.")
    })?;
    if f.write {
        let mut file = std::fs::File::create(&f.path).map_err(|e| merr!("{}", e))?;
        file.write_all(&f.buf).map_err(|e| merr!("{}", e))?;
    }
    Ok(())
}

fn read_line(it: &mut Interp, a: &[Value], keep_newline: bool) -> R {
    check_args(a, 1, 2)?;
    let id = a[0].scalar()? as i32;
    let f = it
        .files
        .get_mut(&id)
        .ok_or_else(|| merr!("Invalid file identifier."))?;
    if f.pos >= f.buf.len() {
        return one(Value::num(-1.0));
    }
    let rest = &f.buf[f.pos..];
    let end = rest
        .iter()
        .position(|b| *b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    let mut line = String::from_utf8_lossy(&rest[..end]).to_string();
    f.pos += end;
    if !keep_newline {
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
    }
    one(Value::chars(&line))
}

fn unescape_delim(s: &str) -> String {
    super::text::unescape(s)
}

fn read_numeric(it: &mut Interp, a: &[Value]) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let p = resolve(it, &a[0].text()?);
    let s = std::fs::read_to_string(&p).map_err(|_| {
        merr!(
            "Unable to find or open '{}'. Check the path and filename or file permissions.",
            p.display()
        )
    })?;
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let parts: Vec<&str> = t
            .split([',', ';', '\t', ' '])
            .filter(|x| !x.is_empty())
            .collect();
        let nums: Vec<f64> = parts
            .iter()
            .map(|x| x.trim().parse::<f64>().unwrap_or(f64::NAN))
            .collect();
        // Skip header rows that are entirely text.
        if rows.is_empty() && nums.iter().all(|x| x.is_nan()) {
            continue;
        }
        rows.push(nums);
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut out = Array::filled(rows.len(), cols, f64::NAN);
    for (i, r) in rows.iter().enumerate() {
        for (j, x) in r.iter().enumerate() {
            out.set(i, j, *x);
        }
    }
    one(Value::mat(out))
}

/// Text arguments to load/save, without option flags like '-v7' or '-mat'.
fn plain_args(a: &[Value]) -> MResult<(Vec<String>, Vec<String>)> {
    let mut names = Vec::new();
    let mut flags = Vec::new();
    for v in a {
        let t = v.text()?;
        if t.starts_with('-') {
            flags.push(t);
        } else {
            names.push(t);
        }
    }
    Ok((names, flags))
}

fn load(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let (mut names, flags) = plain_args(&a)?;
    let file = if names.is_empty() {
        "matlab.mat".to_string()
    } else {
        names.remove(0)
    };
    let mut p = resolve(it, &file);
    if !p.is_file() && p.extension().is_none() {
        p.set_extension("mat");
    }
    let is_ascii = flags.iter().any(|f| f == "-ascii")
        || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mat"));
    if is_ascii {
        let v = read_numeric(it, &[Value::chars(&p.to_string_lossy())])?.remove(0);
        if nargout > 0 {
            return one(v);
        }
        let var = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "data".into());
        it.set_var(&var, v);
        return none();
    }
    let bytes =
        std::fs::read(&p).map_err(|_| merr!("Unable to find file or directory '{}'.", file))?;
    let mut vars = crate::mat::read(&bytes)?;
    if !names.is_empty() {
        vars.retain(|(n, _)| names.contains(n));
        for n in &names {
            if !vars.iter().any(|(v, _)| v == n) {
                it.warn(&format!("Variable '{}' not found.", n));
            }
        }
    }
    if nargout > 0 {
        return one(Value::Struct(StructArr::scalar(vars)));
    }
    for (n, v) in vars {
        it.set_var(&n, v);
    }
    none()
}

fn save(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (mut names, flags) = plain_args(&a)?;
    let file = if names.is_empty() {
        "matlab.mat".to_string()
    } else {
        names.remove(0)
    };
    let mut p = resolve(it, &file);
    if p.extension().is_none() {
        p.set_extension("mat");
    }
    if flags.iter().any(|f| f == "-ascii") {
        let v = names
            .first()
            .and_then(|n| it.get_var(n).cloned())
            .ok_or_else(|| merr!("Specify the variable to save with -ascii."))?;
        return write_numeric(it, &[v, Value::chars(&p.to_string_lossy())], " ");
    }
    let vars: Vec<(String, Value)> = if names.is_empty() {
        it.workspace()
    } else {
        let mut out = Vec::new();
        for n in &names {
            match it.get_var(n) {
                Some(v) => out.push((n.clone(), v.clone())),
                None => return Err(merr!("Variable '{}' not found.", n)),
            }
        }
        out
    };
    let vars: Vec<(String, Value)> = vars
        .into_iter()
        .filter(|(_, v)| !matches!(v, Value::Func(_)))
        .collect();
    let bytes = crate::mat::write(&vars)?;
    std::fs::write(&p, bytes)
        .map_err(|e| merr!("Unable to write file '{}': {}", p.display(), e))?;
    none()
}

fn write_numeric(it: &mut Interp, a: &[Value], delim: &str) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_num()?;
    let p = resolve(it, &a[1].text()?);
    let mut s = String::new();
    for r in 0..m.rows {
        let line: Vec<String> = (0..m.cols)
            .map(|c| crate::display::trim_g(*m.get(r, c), 15))
            .collect();
        s.push_str(&line.join(delim));
        s.push('\n');
    }
    std::fs::write(&p, s).map_err(|e| merr!("{}", e))?;
    none()
}

fn system(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let cmd = a[0].text()?;
    let out = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", &cmd])
            .current_dir(&it.cwd)
            .output()
    } else {
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(&it.cwd)
            .output()
    }
    .map_err(|e| merr!("{}", e))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let status = out.status.code().unwrap_or(-1) as f64;
    if nargout < 2 {
        it.print(&text);
        return one(Value::num(status));
    }
    Ok(vec![Value::num(status), Value::chars(&text)])
}

// ---------- dates ----------

fn days_to_civil(z: i64) -> (i64, u32, u32) {
    // Howard Hinnant's civil-from-days algorithm.
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Seconds since the Unix epoch in local time.
fn local_secs() -> f64 {
    let now = chrono::Local::now();
    now.naive_local().and_utc().timestamp_micros() as f64 / 1e6
}

fn civil_now() -> (i64, u32, u32, u32, u32, f64) {
    let secs = local_secs();
    let days = (secs / 86400.0).floor() as i64;
    let rem = secs - days as f64 * 86400.0;
    let (y, m, d) = days_to_civil(days);
    (
        y,
        m,
        d,
        (rem / 3600.0) as u32,
        ((rem % 3600.0) / 60.0) as u32,
        rem % 60.0,
    )
}

fn datenum_now() -> f64 {
    local_secs() / 86400.0 + 719529.0
}

fn datestr(dn: f64) -> String {
    let days = (dn - 719529.0).floor() as i64;
    let frac = dn - dn.floor();
    let (y, m, d) = days_to_civil(days);
    let secs = (frac * 86400.0).round() as u32;
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{:02}-{}-{} {:02}:{:02}:{:02}",
        d,
        MON[(m - 1) as usize],
        y,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}
