//! dv-filelist: read a VCS/Xcelium file list, report what is wrong with it,
//! and write it back normalized.
//!
//!     dv-filelist [--dialect any|vcs|xcelium] [--json | --args] [--load] <options, e.g. -f top.f>
//!
//! Default output is a summary plus every problem with its file:line.
//! --args prints the normalized compile options, one per line: a flat .f
//! that uvmcheck (and slang) read directly. --json prints everything.
//! --load also compiles the design through dv-model and reports the result.
//! Exit status: 0 clean, 1 problems found, 2 bad usage.

use dv_model::Design;
use std::path::PathBuf;
use std::process::ExitCode;
use sv_source::{Dialect, FileList, Item, ProcessEnv, Severity};

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn origin_json<T>(i: &Item<T>) -> String {
    format!(
        "\"from\":\"{}\",\"line\":{}",
        esc(&i.origin.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default()),
        i.origin.line
    )
}

fn paths_json(items: &[Item<PathBuf>]) -> String {
    let v: Vec<String> = items
        .iter()
        .map(|i| format!("{{\"path\":\"{}\",{}}}", esc(&i.value.display().to_string()), origin_json(i)))
        .collect();
    format!("[{}]", v.join(","))
}

fn to_json(fl: &FileList) -> String {
    let defines: Vec<String> = fl
        .effective_defines()
        .into_iter()
        .map(|(n, v)| match v {
            Some(v) => format!("\"{}\":\"{}\"", esc(&n), esc(&v)),
            None => format!("\"{}\":null", esc(&n)),
        })
        .collect();
    let strs = |items: &[Item<String>]| -> String {
        let v: Vec<String> = items.iter().map(|i| format!("{{\"value\":\"{}\",{}}}", esc(&i.value), origin_json(i))).collect();
        format!("[{}]", v.join(","))
    };
    let diags: Vec<String> = fl
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{{\"severity\":\"{}\",\"message\":\"{}\",\"from\":\"{}\",\"line\":{}}}",
                if d.severity == Severity::Error { "error" } else { "warning" },
                esc(&d.message),
                esc(&d.origin.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default()),
                d.origin.line
            )
        })
        .collect();
    let lists: Vec<String> = fl.lists.iter().map(|p| format!("\"{}\"", esc(&p.display().to_string()))).collect();
    let exts: Vec<String> = fl.library_exts.iter().map(|e| format!("\"{}\"", esc(e))).collect();
    format!(
        "{{\"schema\":\"dv-filelist/1\",\"lists\":[{}],\"sources\":{},\"include_dirs\":{},\"defines\":{{{}}},\
\"library_dirs\":{},\"library_files\":{},\"library_exts\":[{}],\"top\":{},\"timescale\":{},\
\"uvm\":{{\"requested\":{},\"home_or_version\":{}}},\"ignored\":{},\"diagnostics\":[{}]}}",
        lists.join(","),
        paths_json(&fl.sources),
        paths_json(&fl.include_dirs),
        defines.join(","),
        paths_json(&fl.library_dirs),
        paths_json(&fl.library_files),
        exts.join(","),
        strs(&fl.top),
        fl.timescale.as_ref().map(|t| format!("\"{}\"", esc(&t.value))).unwrap_or_else(|| "null".into()),
        fl.uvm.requested,
        fl.uvm.home_or_version.as_ref().map(|v| format!("\"{}\"", esc(v))).unwrap_or_else(|| "null".into()),
        strs(&fl.ignored),
        diags.join(",")
    )
}

fn main() -> ExitCode {
    let mut dialect = Dialect::Any;
    let (mut json, mut args_out, mut load) = (false, false, false);
    let mut rest = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--args" => args_out = true,
            "--load" => load = true,
            "--dialect" => {
                dialect = match it.next().as_deref() {
                    Some("any") => Dialect::Any,
                    Some("vcs") => Dialect::Vcs,
                    Some("xcelium") | Some("xrun") => Dialect::Xcelium,
                    other => {
                        eprintln!("dv-filelist: unknown dialect {other:?} (any, vcs, xcelium)");
                        return ExitCode::from(2);
                    }
                }
            }
            "-h" | "--help" => {
                eprintln!("usage: dv-filelist [--dialect any|vcs|xcelium] [--json | --args] [--load] <options, e.g. -f top.f>");
                return ExitCode::from(2);
            }
            _ => rest.push(a),
        }
    }
    if rest.is_empty() {
        eprintln!("usage: dv-filelist [--dialect any|vcs|xcelium] [--json | --args] [--load] <options, e.g. -f top.f>");
        return ExitCode::from(2);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let fl = FileList::read_args(&rest, &cwd, dialect, &ProcessEnv);
    let mut problems = fl.has_errors();

    if json {
        println!("{}", to_json(&fl));
    } else if args_out {
        for a in fl.to_compile_args() {
            println!("{a}");
        }
    } else {
        println!(
            "{} file list(s): {} sources, {} include dirs, {} defines, {} library dirs, {} library files, {} ignored options",
            fl.lists.len(),
            fl.sources.len(),
            fl.include_dirs.len(),
            fl.effective_defines().len(),
            fl.library_dirs.len(),
            fl.library_files.len(),
            fl.ignored.len()
        );
        if fl.uvm.requested {
            println!(
                "UVM requested from the simulator{}",
                fl.uvm.home_or_version.as_ref().map(|v| format!(" ({v})")).unwrap_or_default()
            );
        }
    }
    for d in &fl.diagnostics {
        let sev = if d.severity == Severity::Error { "error" } else { "warning" };
        eprintln!("{}: {sev}: {}", d.origin, d.message);
    }

    if load {
        match Design::load(fl.to_compile_args()) {
            Ok(d) => {
                eprintln!(
                    "dv-filelist: loaded {} classes from {} files, {} compile errors",
                    d.classes().len(),
                    d.files().len().saturating_sub(1),
                    d.compile_errors()
                );
                problems |= d.compile_errors() > 0;
            }
            Err(e) => {
                eprintln!("dv-filelist: load failed: {e}");
                problems = true;
            }
        }
    }
    if problems {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
