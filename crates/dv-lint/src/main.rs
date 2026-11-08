//! Runs the ported rules over a design, built natively (default) or through
//! slang (`--slang`, model rules only), and prints findings as uvmcheck does:
//!
//!     dv-lint [--slang] [--time] [--history] [--skip-libraries] [--rule ID]... [--preset NAME] [--experimental]
//!             [-top NAME]... [--json FILE] [--list] <compile options>
//!
//! Rules are selected as uvmcheck selects them: by default every profile but
//! advisory and no experimental rule; `--preset NAME` and `--experimental`
//! as uvmcheck's (`--preset all --experimental` runs every rule); `--rule
//! ID` runs that rule whatever its profile. Rules with no uvmcheck twin
//! always run.
//!
//! `--json FILE` also writes the findings as uvmcheck's JSON report (`tool`,
//! `version`, `compileErrors`, `waived`, `violations` with `ruleId`,
//! `ruleName`, `severity`, `file`, `line`, `column`, `message` and any
//! `related` notes).
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
//! Exit status, as uvmcheck's: 1 when an ERROR finding remains (after
//! waivers and re-grades), 0 when only warnings and infos do (or none), 2 for
//! a load or configuration error.
use dv_model::Design;

fn main() {
    let mut slang = false;
    let mut time = false;
    let mut frames = false;
    let mut in_context = false;
    let mut specs = false;
    let mut exprs = false;
    let mut only: Vec<String> = Vec::new();
    let mut preset: Option<String> = None;
    let mut experimental = false;
    let mut no_config = false;
    let mut config_file: Option<String> = None;
    let mut waivers_file: Option<String> = None;
    let mut baseline_file: Option<String> = None;
    let mut update_baseline: Option<String> = None;
    let mut sarif: Option<String> = None;
    let mut html: Option<String> = None;
    let mut fix_rules: Vec<String> = Vec::new();
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        // `--name=value`, as uvmcheck also takes them.
        if let Some(v) = a.strip_prefix("--preset=") {
            preset = Some(v.to_string());
            continue;
        }
        for (flag, slot) in [
            ("--waivers=", &mut waivers_file),
            ("--baseline=", &mut baseline_file),
            ("--update-baseline=", &mut update_baseline),
            ("--sarif=", &mut sarif),
            ("--html=", &mut html),
        ] {
            if let Some(v) = a.strip_prefix(flag) {
                *slot = Some(v.to_string());
            }
        }
        if ["--waivers=", "--baseline=", "--update-baseline=", "--sarif=", "--html="].iter().any(|p| a.starts_with(p)) {
            continue;
        }
        if let Some(v) = a.strip_prefix("--config=") {
            config_file = Some(v.to_string());
            continue;
        }
        if let Some(v) = a.strip_prefix("--json=") {
            let _ = JSON_OUT.set(v.to_string());
            continue;
        }
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
            // Rule selection as uvmcheck's (default: every profile but advisory).
            "--preset" => preset = it.next(),
            "--experimental" => experimental = true,
            // uvmcheck options with nothing to do here: no project config
            // or result cache yet.
            // The project configuration (docs/guide/project_config.md).
            "--no-config" => no_config = true,
            "--config" => config_file = it.next(),
            // A waiver file, a baseline to filter by, a baseline to write.
            "--waivers" => waivers_file = it.next(),
            "--baseline" => baseline_file = it.next(),
            "--update-baseline" => update_baseline = it.next(),
            // A SARIF 2.1.0 report.
            "--sarif" => sarif = it.next(),
            // The HTML dashboard.
            "--html" => html = it.next(),
            // Apply fix-its: --fix rewrites files, --fix-dry-run prints the
            // diff; --unsafe-fixes admits fixes that can change behaviour;
            // --fix-rule limits them to rules (comma-separated IDs or names).
            "--fix" => FIX.write().map(|mut f| f.on = true).unwrap_or(()),
            "--fix-dry-run" => FIX.write().map(|mut f| f.dry_run = true).unwrap_or(()),
            "--unsafe-fixes" => FIX.write().map(|mut f| f.unsafe_ok = true).unwrap_or(()),
            "--fix-rule" => fix_rules.extend(
                it.next().unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            ),
            // No result cache here.
            "--no-cache" => {}
            // Also write the findings as uvmcheck's JSON report.
            "--json" => {
                let path = it.next().unwrap_or_else(|| {
                    eprintln!("error: --json needs a file");
                    std::process::exit(2);
                });
                let _ = JSON_OUT.set(path);
            }
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
    if !no_config {
        let first = args.iter().find(|a| !a.starts_with(['-', '+'])).map(String::as_str);
        if let Some(file) = dv_lint::project::find(config_file.as_deref(), first) {
            let cfg = dv_lint::project::Config::load(&file).unwrap_or_else(|e| fail(e));
            for w in &cfg.warnings {
                eprintln!("Warning: {}: {w}", file.display());
            }
            if !cfg.unapplied.is_empty() {
                eprintln!(
                    "note: {}: {} not applied by the native engine yet (run uvmcheck-slang for them)",
                    file.display(),
                    cfg.unapplied.join(", ")
                );
            }
            args.extend(cfg.compile_args.iter().cloned());
            dv_lint::options::set(&cfg.options);
            dv_lint::options::set_style(&cfg.style);
            dv_lint::options::set_naming(&cfg.naming);
            let _ = CONFIG.set(cfg);
        }
    }
    let sup =
        dv_lint::suppress::Suppress::load(CONFIG.get(), waivers_file.as_deref(), baseline_file.as_deref(), no_config)
            .unwrap_or_else(|e| fail(e));
    for w in &sup.warnings {
        eprintln!("Warning: {w}");
    }
    let _ = SUPPRESS.set(sup);
    if let Some(u) = update_baseline {
        let _ = UPDATE_BASELINE.set(u);
    }
    if let Some(s) = sarif {
        let _ = SARIF_OUT.set(s);
    }
    if let Some(h) = html {
        let _ = HTML_OUT.set(h);
    }
    let headers = if in_context { in_owner(&mut args) } else { Vec::new() };
    let selected = dv_lint::rule_selection(preset.as_deref(), experimental).unwrap_or_else(|e| fail(e));
    // A rule named with --rule runs whatever its profile.
    let user_keep = |id: &str| if only.is_empty() { selected(id) } else { only.iter().any(|o| o == id) };
    let fixing = FIX.read().is_ok_and(|f| f.on || f.dry_run);
    if let Ok(mut f) = FIX.write() {
        f.only = fix_rules.clone();
        f.compile_errors_selected = user_keep("SV-CMP-001");
    }
    // A fix never touches a file with compile errors: find them whatever the
    // selection (the findings are dropped again when not selected).
    let keep = |id: &str| user_keep(id) || (fixing && id == "SV-CMP-001");
    if !slang && !exprs && !specs {
        // The design is built while the rules' shared caches start.
        let (trees, parse_ms) = sv_sema::parse_args(&args).unwrap_or_else(|e| fail(e.to_string()));
        let t = std::time::Instant::now();
        let (design, (files, found)) = dv_lint::lint_built(|| sv_sema::design_of(&trees, parse_ms), &trees, keep);
        report(&design, time, frames, t, &files, only_in(&headers, &files, &found));
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
    report(&design, time, frames, t, &files, only_in(&headers, &files, &found));
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
/// `--fix` and its options.
#[derive(Default)]
struct FixOpts {
    on: bool,
    dry_run: bool,
    unsafe_ok: bool,
    only: Vec<String>,
    compile_errors_selected: bool,
}
static FIX: std::sync::RwLock<FixOpts> = std::sync::RwLock::new(FixOpts {
    on: false,
    dry_run: false,
    unsafe_ok: false,
    only: Vec::new(),
    compile_errors_selected: false,
});

/// --fix runs pass after pass until nothing more changes (a fix can uncover
/// another finding; overlapping fixes wait for the rewritten text); each pass
/// is a fresh process, as uvmcheck's.
const MAX_FIX_PASSES: u32 = 10;

/// The waivers and baseline in use, and where `--update-baseline` writes.
static SUPPRESS: std::sync::OnceLock<dv_lint::suppress::Suppress> = std::sync::OnceLock::new();
static UPDATE_BASELINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
/// `--sarif FILE`.
static SARIF_OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
/// `--html FILE`.
static HTML_OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The project configuration in use, if any.
static CONFIG: std::sync::OnceLock<dv_lint::project::Config> = std::sync::OnceLock::new();

/// `--json FILE`: where to write the JSON report.
static JSON_OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// A JSON string literal.
fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The findings in uvmcheck's JSON report form (`--json`): `tool`,
/// `version`, `compileErrors`, `waived` and one `violations` entry per
/// finding with its rule, severity, position, message and notes.
fn json_report(files: &[String], found: &[dv_lint::Finding], waived: usize) -> String {
    let mut out = String::from("{\n");
    let compile = found.iter().filter(|f| f.rule == "SV-CMP-001").count();
    out.push_str(&format!(
        "  \"tool\": \"dv-lint\",\n  \"version\": {},\n  \"compileErrors\": {compile},\n  \"waived\": {waived},\n  \"violations\": [",
        json_str(env!("CARGO_PKG_VERSION"))
    ));
    let mut pos = dv_lint::fix::Positions::default();
    for (i, f) in found.iter().enumerate() {
        let (file, line, column) = f.position_in(files);
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&format!(
            "    {{\"ruleId\": {}, \"ruleName\": {}, \"severity\": {}, \"file\": {}, \"line\": {line}, \"column\": {column}, \"message\": {}",
            json_str(f.rule),
            json_str(f.name),
            json_str(f.severity.as_str()),
            json_str(file),
            json_str(&f.message)
        ));
        if !f.fixes.is_empty() {
            let fixes: Vec<String> = f
                .fixes
                .iter()
                .map(|x| {
                    let (l1, c1) = pos.at(&x.path, x.start);
                    let (l2, c2) = pos.at(&x.path, x.end);
                    format!(
                        "{{\"description\": {}, \"replacement\": {}, \"safety\": \"{}\", \"line\": {l1}, \"column\": {c1}, \"endLine\": {l2}, \"endColumn\": {c2}}}",
                        json_str(&x.description),
                        json_str(&x.text),
                        x.safety.as_str()
                    )
                })
                .collect();
            out.push_str(&format!(", \"fixes\": [{}]", fixes.join(", ")));
        }
        if !f.related.is_empty() {
            let notes: Vec<String> = f
                .related
                .iter()
                .map(|(loc, note)| {
                    let nfile = files.get(loc.file as usize).map(String::as_str).unwrap_or("");
                    format!(
                        "{{\"file\": {}, \"line\": {}, \"column\": {}, \"message\": {}}}",
                        json_str(nfile),
                        loc.line,
                        loc.column,
                        json_str(note)
                    )
                })
                .collect();
            out.push_str(&format!(", \"related\": [{}]", notes.join(", ")));
        }
        out.push('}');
    }
    out.push_str(if found.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
    out
}

fn report(
    design: &Design,
    time: bool,
    frames: bool,
    t: std::time::Instant,
    files: &[String],
    mut found: Vec<dv_lint::Finding>,
) -> ! {
    let fix = FIX.read().map(|f| (f.on, f.dry_run, f.unsafe_ok, f.only.clone(), f.compile_errors_selected)).ok();
    let (fix_on, fix_dry, fix_unsafe, fix_only, cmp_selected) = fix.unwrap_or((false, false, false, Vec::new(), true));
    let mut broken = std::collections::HashSet::new();
    if fix_on || fix_dry {
        for f in found.iter().filter(|f| f.rule == "SV-CMP-001") {
            broken.insert(f.position_in(files).0.to_string());
        }
        if !cmp_selected {
            found.retain(|f| f.rule != "SV-CMP-001");
        }
    }
    dv_lint::fix::guard_macro_calls(&mut found);
    if let Some(c) = CONFIG.get() {
        c.apply(&mut found, files);
    }
    if let (Some(path), Some(s)) = (UPDATE_BASELINE.get(), SUPPRESS.get()) {
        match s.save_baseline(std::path::Path::new(path), &found, files) {
            Ok(()) => eprintln!("[dv-lint] Saved {} findings to baseline: {path}", found.len()),
            Err(e) => {
                eprintln!("error: cannot write the baseline {e}");
                std::process::exit(2);
            }
        }
    }
    let waived = dv_lint::inline_waived() + SUPPRESS.get().map_or(0, |s| s.apply(&mut found, files));
    if fix_on || fix_dry {
        let pass = dv_lint::fix::apply(&found, &fix_only, fix_unsafe, fix_dry || !fix_on, &broken);
        if fix_dry || !fix_on {
            print!("{}{}", pass.diffs, pass.summary);
        } else if !pass.written.is_empty() {
            rerun_fix_pass(&pass);
        } else {
            print!("{}", pass.summary);
        }
    }
    let found = &found[..];
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
    if let Some(path) = HTML_OUT.get() {
        if let Err(e) = std::fs::write(path, dv_lint::report::html(files, found)) {
            eprintln!("error: cannot write {path}: {e}");
            std::process::exit(2);
        }
    }
    if let Some(path) = SARIF_OUT.get() {
        if let Err(e) = std::fs::write(path, dv_lint::report::sarif(files, found)) {
            eprintln!("error: cannot write {path}: {e}");
            std::process::exit(2);
        }
    }
    if let Some(path) = JSON_OUT.get() {
        if let Err(e) = std::fs::write(path, json_report(files, found, waived)) {
            eprintln!("error: cannot write {path}: {e}");
            std::process::exit(2);
        }
    }
    // As uvmcheck: warnings and infos alone do not fail the run.
    std::process::exit(if found.iter().any(|f| f.severity == dv_lint::Severity::Error) { 1 } else { 0 });
}

#[cfg(feature = "slang")]
fn load_slang(args: &[String]) -> Result<Design, String> {
    Design::load(args).map_err(|e| e.to_string())
}

#[cfg(not(feature = "slang"))]
fn load_slang(_: &[String]) -> Result<Design, String> {
    Err("built without the slang feature".into())
}

/// After a --fix pass that rewrote files: unless the fixes undo each other or
/// the pass limit is reached, print the pass summary and run the next pass
/// (this binary again, with the pass number and the file states seen so far
/// in the environment); it reports on the fixed text.
fn rerun_fix_pass(pass: &dv_lint::fix::PassResult) {
    let n: u32 = std::env::var("DV_LINT_FIX_PASS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let mut seen: std::collections::BTreeSet<String> = std::env::var("DV_LINT_FIX_STATES")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let keys: Vec<String> = pass.written.iter().map(|(p, h)| format!("{p}#{h:016x}")).collect();
    let cycled = keys.iter().all(|k| seen.contains(k));
    if cycled || n >= MAX_FIX_PASSES {
        eprintln!(
            "Warning: --fix: {} after {n} pass(es); stopped. Review the remaining findings by hand.",
            if cycled { "fixes undo each other" } else { "fixes still changing files" }
        );
        print!("{}", pass.summary);
        return;
    }
    print!("{}", pass.summary);
    use std::io::Write;
    let _ = std::io::stdout().flush();
    seen.extend(keys);
    let states: Vec<String> = seen.into_iter().collect();
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("dv-lint"));
    let status = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("DV_LINT_FIX_PASS", (n + 1).to_string())
        .env("DV_LINT_FIX_STATES", states.join(","))
        .status();
    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(2)),
        Err(e) => eprintln!("Warning: --fix: could not start pass {}: {e}; run --fix again to continue.", n + 1),
    }
}
