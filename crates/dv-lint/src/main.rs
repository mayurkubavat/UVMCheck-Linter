//! Runs the ported rules over a design, built natively (default) or through
//! slang (`--slang`, model rules only), and prints findings as uvmcheck does:
//!
//!     dv-lint [--slang] [--time] [--history] [--skip-libraries] [--rule ID]... [-top NAME]... [--list] <compile options>
//!
//! `--in-context` lints each header given as a source (`.svh`, `.vh`, or a
//! `.sv` with no design unit) inside
//! the file that includes it, found by indexing the repository's includes
//! (`sv_source::owners`), and reports only the header's findings.
//! `--frames` prints, under each finding and note, its source line with a
//! caret under the flagged token (as uvmcheck's text report does).
//! `--list` prints every rule (`id name model|syntax`, then `native` for a
//! rule with no uvmcheck twin); `-top` names the
//! modules elaboration starts from; `--history` lets SV-EVO-001/002 read git
//! history. Modules the sources instantiate but do not define are looked up
//! in `-v` library files and `-y` directories (`+libext+` extensions), as
//! VCS does; `--skip-libraries` reports nothing in those files. For the gates: `--specs` prints how often each generic class is
//! visited, `--exprs` every expression's type (the form of
//! `tools/oracles/exprdump.cpp`).
//!
//! Rules run on one thread per core (`DV_LINT_JOBS=N` to choose; 1 runs
//! them serially); the output does not depend on it.
//!
//! Exit status: 0 clean, 1 findings, 2 load error.
use dv_model::Design;

fn main() {
    let mut slang = false;
    let mut time = false;
    let mut frames = false;
    let mut in_context = false;
    let mut specs = false;
    let mut exprs = false;
    let mut only: Vec<String> = Vec::new();
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--slang" => slang = true,
            "--time" => time = true,
            // Under each finding and note, its source line with a caret.
            "--frames" => frames = true,
            // A header given as a source is linted inside the file that includes it.
            "--in-context" => in_context = true,
            "--specs" => specs = true,
            // Each expression's type, as tools/oracles/exprdump.cpp prints slang's.
            "--exprs" => exprs = true,
            "--rule" => only.extend(it.next()),
            // Read git history for SV-EVO-001 and SV-EVO-002.
            "--history" => dv_lint::set_history(true),
            // Report nothing in -v/-y library files.
            "--skip-libraries" => dv_lint::set_skip_libraries(true),
            // The design's tops, as slang's --top: modules no top reaches
            // are not elaborated.
            "-top" | "--top" => {
                let t: Vec<String> = it.next().into_iter().collect();
                dv_lint::add_tops(&t);
            }
            "--list" => {
                for r in dv_lint::rules() {
                    let kind = if r.is_syntax() { "syntax" } else { "model" };
                    let native = if dv_lint::NATIVE_ONLY.contains(&r.id) { " native" } else { "" };
                    println!("{} {} {}{native}", r.id, r.name, kind);
                    for (id, name) in dv_lint::sub_rules(r.id) {
                        println!("{id} {name} {kind}");
                    }
                }
                return;
            }
            _ => args.push(a),
        }
    }
    let fail = |e: String| -> ! {
        eprintln!("error: {e}");
        std::process::exit(2);
    };
    let headers = if in_context { in_owner(&mut args) } else { Vec::new() };
    let keep = |id: &str| only.is_empty() || only.iter().any(|o| o == id);
    if !slang && !exprs && !specs {
        // The design is built while the rules' shared caches start.
        let (trees, parse_ms) = sv_sema::parse_args(&args).unwrap_or_else(|e| fail(e.to_string()));
        let t = std::time::Instant::now();
        let (design, (files, found)) = dv_lint::lint_built(|| sv_sema::design_of(&trees, parse_ms), &trees, keep);
        report(&design, time, frames, t, &files, &only_in(&headers, &files, &found));
    }
    let loaded = if slang {
        load_slang(&args).map(|d| (d, Vec::new()))
    } else {
        sv_sema::load_with_trees(&args).map_err(|e| e.to_string())
    };
    let (design, trees) = loaded.unwrap_or_else(|e| fail(e));
    if exprs {
        for l in dv_lint::expr_types::expr_dump(design.files().to_vec(), &trees) {
            println!("{l}");
        }
        return;
    }
    if specs {
        // How often the AST walk visits each generic class (`name@file:line`).
        for l in dv_lint::spec_report(&design, &trees) {
            println!("{l}");
        }
        return;
    }
    let t = std::time::Instant::now();
    let (files, found) = dv_lint::lint_all(&design, &trees, keep);
    report(&design, time, frames, t, &files, &only_in(&headers, &files, &found));
}

/// `--in-context`: each header source (`.svh`, `.vh`, ...) is replaced by
/// the compile root that includes it (see `sv_source::owners`), with the
/// folders on the way as include directories. Returns the headers, whose
/// findings are the ones to report; a header nothing includes stays a
/// source of its own.
fn in_owner(args: &mut Vec<String>) -> Vec<std::path::PathBuf> {
    use sv_source::owners::{is_header, search_root, IncludeIndex};
    let incdirs: Vec<std::path::PathBuf> = args
        .iter()
        .filter_map(|a| a.strip_prefix("+incdir+"))
        .flat_map(|d| d.split('+').filter(|x| !x.is_empty()).map(std::path::PathBuf::from).collect::<Vec<_>>())
        .collect();
    let mut headers = Vec::new();
    let mut out: Vec<String> = Vec::new();
    let mut extra: Vec<String> = Vec::new();
    let mut prev = String::new();
    for a in std::mem::take(args) {
        let p = std::path::Path::new(&a);
        let takes_value = matches!(prev.as_str(), "-f" | "-F" | "-v" | "-y" | "-top" | "--top" | "--rule");
        prev = a.clone();
        if takes_value || a.starts_with(['-', '+']) || !p.is_file() || !(is_header(p) || no_design_unit(p)) {
            out.push(a);
            continue;
        }
        let idx = IncludeIndex::scan(&[search_root(p)], &incdirs);
        let canon = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        match idx.owners(p).into_iter().next() {
            Some(o) => {
                eprintln!(
                    "dv-lint: {} is linted inside {}{}",
                    a,
                    o.root.display(),
                    o.package.as_ref().map(|k| format!(" (package {k})")).unwrap_or_default()
                );
                for d in &o.include_dirs {
                    extra.push(format!("+incdir+{}", d.display()));
                }
                let root = o.root.display().to_string();
                if !out.contains(&root) && !extra.contains(&root) {
                    extra.push(root);
                }
            }
            None => {
                eprintln!("dv-lint: nothing includes {a}; linting it alone");
                out.push(a);
            }
        }
        headers.push(canon);
    }
    out.extend(extra);
    *args = out;
    headers
}

/// A `.sv` file with no module, package, interface or program, such as a
/// class meant to be included in a package (core-v-verif names those .sv).
fn no_design_unit(p: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(p) else { return false };
    !text.lines().any(|l| {
        let l = l.trim_start();
        ["module", "package", "interface", "program", "macromodule", "primitive", "config"]
            .iter()
            .any(|k| l.strip_prefix(k).is_some_and(|r| r.starts_with([' ', '\t'])))
    })
}

/// The findings in `headers` (all of them when there are none).
fn only_in(headers: &[std::path::PathBuf], files: &[String], found: &[dv_lint::Finding]) -> Vec<dv_lint::Finding> {
    if headers.is_empty() {
        return found.to_vec();
    }
    let canon = |f: &str| std::path::Path::new(f).canonicalize().ok();
    found.iter().filter(|f| canon(f.position_in(files).0).is_some_and(|p| headers.contains(&p))).cloned().collect()
}

/// Prints the findings (and with `--time` the stage times) and exits.
fn report(
    design: &Design,
    time: bool,
    frames: bool,
    t: std::time::Instant,
    files: &[String],
    found: &[dv_lint::Finding],
) -> ! {
    let mut sources = dv_lint::frames::Sources::default();
    if time {
        let tm = design.timings();
        eprintln!(
            "parse {:.0} ms, elaborate {:.0} ms, model {:.0} ms, copy {:.0} ms, rules {:.0} ms",
            tm.parse_ms,
            tm.elaborate_ms,
            tm.model_ms,
            tm.copy_ms,
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    for f in found {
        let (file, line, column) = f.position_in(files);
        println!("{}:{}:{}: [{}] {} ({}): {}", file, line, column, f.severity.as_str(), f.rule, f.name, f.message);
        if frames {
            print!("{}", sources.frame(file, line, column));
        }
        for (loc, note) in &f.related {
            let nfile = files.get(loc.file as usize).map(String::as_str).unwrap_or("");
            println!("    note: {}:{}:{}: {}", nfile, loc.line, loc.column, note);
            if frames {
                print!("{}", sources.frame(nfile, loc.line, loc.column));
            }
        }
    }
    std::process::exit(if found.is_empty() { 0 } else { 1 });
}

#[cfg(feature = "slang")]
fn load_slang(args: &[String]) -> Result<Design, String> {
    Design::load(args).map_err(|e| e.to_string())
}

#[cfg(not(feature = "slang"))]
fn load_slang(_: &[String]) -> Result<Design, String> {
    Err("built without the slang feature".into())
}
