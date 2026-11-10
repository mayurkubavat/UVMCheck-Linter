//! The command line of `uvmcheck` and `dv-lint` (one program, two names).
//! Runs the ported rules over a design, built natively (default) or through
//! slang (`--slang`, model rules only), and prints findings as uvmcheck does:
//!
//! ```text
//! uvmcheck [--slang] [--time] [--history] [--skip-libraries] [--rule ID]... [--preset NAME] [--experimental]
//!          [-top NAME]... [--json FILE] [--list] <compile options>
//! ```
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
//! caret under the flagged token (as uvmcheck's text report does); a
//! terminal gets them without asking, and `--no-frames` turns them off.
//! `--list` prints every rule (`id name model|syntax`, then `native` for a
//! rule with no uvmcheck twin); `-top` names the
//! modules elaboration starts from; `--history` lets SV-EVO-001/002 read git
//! history. Modules the sources instantiate but do not define are looked up
//! in `-v` library files and `-y` directories (`+libext+` extensions), as
//! VCS does; `--skip-libraries` reports nothing in those files. For the gates: `--specs` prints how often each generic class is
//! visited, `--exprs` every expression's type (the form of
//! `tools/oracles/exprdump.cpp`).
//!
//! uvmcheck's remaining options (see `dv_lint::cli`): `--explain RULE`;
//! `--changed-since REF` (with `--strict-new-files`); `--waiver-report`,
//! `--report-unused-waivers` and `--strict-waivers` (exit 4); the result
//! cache in `.uvmcheck_cache/native/` (`--no-cache`, `--cache-dir DIR`,
//! `--clear-cache`; a run is replayed only when nothing it read changed);
//! `--map FILE` (the testbench map, HTML or JSON); `--init`; and
//! `--python-rules PATH` (rules written against the Python SDK).
//!
//! Rules run on one thread per core (`DV_LINT_JOBS=N` to choose; 1 runs
//! them serially); the output does not depend on it.
//!
//! Exit status, as uvmcheck's: 1 when an ERROR finding remains (after
//! waivers and re-grades), 0 when only warnings and infos do (or none), 2 for
//! a load or configuration error.
use dv_model::Design;

/// The command line: `dv-lint` and `uvmcheck` are this one program.
pub fn run() {
    sv_source::exit_on_closed_pipe();
    let mut slang = false;
    let mut time = false;
    // The source line under each finding: on a terminal unless --no-frames,
    // elsewhere only with --frames (a log or a script gets one line each).
    let mut frames = std::io::IsTerminal::is_terminal(&std::io::stdout());
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
    let mut listing: Option<Listing> = None;
    let mut json_bare = false;
    // --explain, --changed-since, waiver hygiene, the cache, --map, --init,
    // --python-rules (dv_lint::cli).
    let mut extra = dv_lint::cli::Extra::default();
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match extra.take(&a, &mut it) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
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
            "-h" | "--help" => {
                print!("{}", dv_lint::help::help());
                return;
            }
            "--version" => {
                println!("{}", dv_lint::help::version());
                return;
            }
            "--lsp" => serve_editor(),
            "--slang" => slang = true,
            "--time" => time = true,
            // Under each finding and note, its source line with a caret.
            "--frames" => frames = true,
            "--no-frames" => frames = false,
            // A header given as a source is linted inside the file that includes it.
            "--in-context" => in_context = true,
            "--specs" => specs = true,
            // Each expression's type, as tools/oracles/exprdump.cpp prints slang's.
            "--exprs" => exprs = true,
            "--rule" => only.extend(it.next()),
            // Rule selection as uvmcheck's (default: every profile but advisory).
            "--preset" => preset = it.next(),
            "--experimental" => experimental = true,
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
            // Also write the findings as uvmcheck's JSON report.
            "--json" => {
                // Alone it means standard output, for the two listings.
                let path = it.next().unwrap_or_else(|| {
                    json_bare = true;
                    "-".to_string()
                });
                let _ = JSON_OUT.set(path);
            }
            // The catalog and the guide, as text or with --json.
            "--list-rules" => listing = Some(Listing::Rules),
            "--list-guidelines" => listing = Some(Listing::Guidelines),
            // Read git history for SV-EVO-001 and SV-EVO-002.
            "--history" => dv_lint::set_history(true),
            // Report nothing in -v/-y library files.
            "--skip-libraries" => dv_lint::set_skip_libraries(true),
            // Report in the UVM library too (it is left out beside user code).
            "--report-library" => REPORT_LIBRARY.store(true, std::sync::atomic::Ordering::Relaxed),
            // The design's tops, as slang's --top: modules no top reaches
            // are not elaborated.
            "-top" | "--top" => {
                let t: Vec<String> = it.next().into_iter().collect();
                dv_lint::add_tops(&t);
            }
            "--list" => {
                for r in dv_lint::rules() {
                    let kind = if r.is_syntax() { "syntax" } else { "model" };
                    let native = if r.native { " native" } else { "" };
                    println!("{} {} {}{native}", r.id, r.name, kind);
                    for s in dv_lint::sub_rules(r.id) {
                        println!("{} {} {kind}", s.id, s.finding_name);
                    }
                }
                return;
            }
            _ => args.push(a),
        }
    }
    let fail = |e: String| -> ! {
        eprintln!("error: {e}");
        // A mistyped command line: say where the options are listed.
        if e == "no sources given" || e.starts_with("unknown command line argument") {
            eprintln!("{}", dv_lint::help::hint());
        }
        std::process::exit(2);
    };
    if let Some(l) = listing {
        list(l, JSON_OUT.get().map(String::as_str)).unwrap_or_else(|e| fail(e));
        return;
    }
    if json_bare {
        fail("--json needs a file".to_string());
    }
    extra.validate().unwrap_or_else(|e| fail(e));
    if let Some(q) = &extra.explain {
        use std::io::IsTerminal;
        print!("{}", dv_lint::explain::explain(q, std::io::stdout().is_terminal()).unwrap_or_else(|e| fail(e)));
        return;
    }
    if extra.init {
        init_project(&args);
    }
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
    let root = CONFIG
        .get()
        .map(|c| c.root.clone())
        .filter(|r| !r.as_os_str().is_empty())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    if extra.clear_cache {
        dv_lint::cache::clear(&extra.cache_folder(&root));
    }
    let cache_wanted = !extra.no_cache
        && !extra.hygiene()
        && extra.changed_since.is_none()
        && extra.map.is_none()
        && update_baseline.is_none()
        && !time
        && !std::env::args().any(|a| a == "--history")
        && !FIX.read().is_ok_and(|f| f.on || f.dry_run);
    let cache = cache_wanted.then(|| {
        let mut inputs: Vec<std::path::PathBuf> = Vec::new();
        if let Some(c) = CONFIG.get() {
            inputs.push(c.file.clone());
            inputs.extend(c.layers.iter().map(std::path::PathBuf::from));
        }
        let dot = root.join(".uvmcheck");
        match &waivers_file {
            Some(w) => inputs.push(w.into()),
            None if !no_config => inputs.push(dot.join("waivers.json")),
            None => {}
        }
        match &baseline_file {
            Some(b) => inputs.push(b.into()),
            None if !no_config => inputs.push(dot.join("baseline.json")),
            None => {}
        }
        inputs.extend(extra.python_rules.iter().map(std::path::PathBuf::from));
        let cwd = std::env::current_dir().unwrap_or_default();
        let lists = sv_source::FileList::read_args(&args, &cwd, sv_source::Dialect::Any, &sv_source::ProcessEnv);
        inputs.extend(lists.lists);
        let mut key: Vec<String> = vec![cwd.display().to_string()];
        key.extend(lint_args());
        key.push("|".into());
        key.extend(args.iter().cloned());
        dv_lint::cache::Cache::new(&extra.cache_folder(&root), &key, &inputs)
    });
    let _ = ROOT.set(root);
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
    // --preset, else the project's `preset`, else the default profiles.
    let config_preset = CONFIG.get().and_then(|c| c.preset.clone());
    let selected = dv_lint::rule_selection(preset.as_deref().or(config_preset.as_deref()), experimental)
        .unwrap_or_else(|e| fail(e));
    // Rules the project config configures run too, unless --preset narrows
    // the run from the command line.
    let configured: std::collections::HashSet<&'static str> = match (CONFIG.get(), &preset) {
        (Some(c), None) => dv_lint::rules().into_iter().filter(|r| c.selects(r.id, r.name)).map(|r| r.id).collect(),
        _ => Default::default(),
    };
    // So do the rules --fix-rule names: asking for a rule's fixes asks for
    // the rule.
    let fix_named: std::collections::HashSet<&'static str> = dv_lint::rules()
        .into_iter()
        .filter(|r| {
            let named = |id: &str, name: &str| fix_rules.iter().any(|f| f == id || f == name);
            named(r.id, r.name)
                || dv_lint::sub_rules(r.id).iter().any(|s| named(s.id, s.name) || named(s.id, s.finding_name))
        })
        .map(|r| r.id)
        .collect();
    // A rule named with --rule runs whatever its profile.
    let user_keep = |id: &str| {
        if only.is_empty() {
            selected(id) || configured.contains(id) || fix_named.contains(id)
        } else {
            only.iter().any(|o| o == id)
        }
    };
    let fixing = FIX.read().is_ok_and(|f| f.on || f.dry_run);
    if let Ok(mut f) = FIX.write() {
        f.only = fix_rules.clone();
        f.compile_errors_selected = user_keep("SV-CMP-001");
    }
    // A fix never touches a file with compile errors: find them whatever the
    // selection (the findings are dropped again when not selected).
    let keep = |id: &str| user_keep(id) || (fixing && id == "SV-CMP-001");
    let _ = ACTIVE.set(dv_lint::rules().into_iter().map(|r| r.id).filter(|id| user_keep(id)).collect());
    let _ = EXTRA.set(extra.clone());
    let _ = LIBRARY.set(dv_lint::library::Library::of_args(&args));
    if !slang && !exprs && !specs {
        let looked = cache.as_ref().map(dv_lint::cache::Cache::lookup);
        if let Some(Err(changed)) = &looked {
            if !changed.is_empty() {
                // A full run: only a complete hit is replayed.
                eprintln!("[dv-lint] cache miss: {} changed", changed.join(", "));
            }
        }
        if let Some(Ok(hit)) = looked {
            eprintln!(
                "[dv-lint] cache: {} compilation unit{} served from cache",
                hit.units,
                if hit.units == 1 { "" } else { "s" }
            );
            dv_lint::set_inline_waived(hit.inline_waived);
            let _ = USER_FILES.set(user_files(&hit.files));
            let t = std::time::Instant::now();
            report(None, time, frames, t, &hit.files, only_in(&headers, &hit.files, &hit.found));
        }
        // The design is built while the rules' shared caches start.
        let (trees, parse_ms) = sv_sema::parse_args(&args).unwrap_or_else(|e| fail(e.to_string()));
        let t = std::time::Instant::now();
        let (design, (mut files, mut found)) =
            dv_lint::lint_built(|| sv_sema::design_of(&trees, parse_ms), &trees, keep);
        if let Some(rules) = &extra.python_rules {
            eprintln!("[dv-lint] Executing custom Python rules from: {rules}");
            let options = CONFIG.get().map_or(&[][..], |c| &c.rule_options[..]);
            let mut py = dv_lint::plugins::run(rules, &design, &trees, &mut files, options);
            dv_lint::apply_inline_waivers(&mut py, &files);
            found.extend(py);
            found.sort_by(|a, b| (a.position_in(&files), a.rule).cmp(&(b.position_in(&files), b.rule)));
        }
        if extra.map.is_some() {
            let _ = MAP.set(dv_lint::tbmap::Map::new(&design, &trees, &files));
        }
        let _ = USER_FILES.set(user_files(&files));
        if let Some(c) = &cache {
            let mut deps: Vec<std::path::PathBuf> = trees.iter().flat_map(|t| t.files.iter().cloned()).collect();
            deps.extend(files.iter().filter(|f| !f.is_empty()).map(std::path::PathBuf::from));
            deps.retain(|d| d.is_file());
            if c.store(&deps, trees.len(), &files, &found, dv_lint::inline_waived()) {
                eprintln!(
                    "[dv-lint] cache: stored {} compilation unit{}",
                    trees.len(),
                    if trees.len() == 1 { "" } else { "s" }
                );
            }
        }
        report(Some(&design), time, frames, t, &files, only_in(&headers, &files, &found));
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
    let _ = USER_FILES.set(user_files(&files));
    report(Some(&design), time, frames, t, &files, only_in(&headers, &files, &found));
}

/// `--lsp`: the language server is `dv-lsp`, installed next to this binary
/// (else on PATH). It takes over standard input and output.
/// What `--list-rules` and `--list-guidelines` print.
#[derive(Clone, Copy)]
enum Listing {
    Rules,
    Guidelines,
}

/// Prints a listing: as text, or as JSON to `json` (`-`: standard output).
fn list(what: Listing, json: Option<&str>) -> Result<(), String> {
    use dv_lint::catalog;
    let text = match (what, json.is_some()) {
        (Listing::Rules, true) => catalog::rules_json(),
        (Listing::Rules, false) => catalog::rules_text(),
        (Listing::Guidelines, true) => catalog::guidelines_json(),
        (Listing::Guidelines, false) => catalog::guidelines_text(),
    };
    match json {
        Some(path) if path != "-" => std::fs::write(path, text).map_err(|e| format!("cannot write {path}: {e}")),
        _ => {
            use std::io::Write;
            // A closed pipe (`| head`) is not an error.
            let _ = std::io::stdout().write_all(text.as_bytes());
            Ok(())
        }
    }
}

fn serve_editor() -> ! {
    let name = if cfg!(windows) { "dv-lsp.exe" } else { "dv-lsp" };
    let beside = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).map(|e| e.with_file_name(name));
    let server = beside.filter(|p| p.is_file()).unwrap_or_else(|| name.into());
    match std::process::Command::new(&server).status() {
        Ok(s) => std::process::exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("error: cannot start the language server {}: {e}", server.display());
            std::process::exit(2);
        }
    }
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

/// The options of dv_lint::cli, the rules that run, the project root, the
/// files whose waiver comments count, and the --map model.
static EXTRA: std::sync::OnceLock<dv_lint::cli::Extra> = std::sync::OnceLock::new();
static ACTIVE: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
static USER_FILES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
static MAP: std::sync::OnceLock<dv_lint::tbmap::Map> = std::sync::OnceLock::new();

/// The command line without the options that only say where reports go
/// (or how the cache is used), for the cache key: a run that writes its JSON
/// to another file replays the same findings.
fn lint_args() -> Vec<String> {
    const VALUED: [&str; 6] = ["--json", "--sarif", "--html", "--map", "--update-baseline", "--cache-dir"];
    const FLAGS: [&str; 4] = ["--frames", "--no-frames", "--clear-cache", "--no-cache"];
    let mut out = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if VALUED.contains(&a.as_str()) {
            it.next();
        } else if !FLAGS.contains(&a.as_str()) && !VALUED.iter().any(|v| a.starts_with(&format!("{v}="))) {
            out.push(a);
        }
    }
    out
}

/// Files whose waiver comments the hygiene report reads: not library or
/// excluded, not the UVM test stub.
fn user_files(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|f| !f.is_empty() && !f.contains("uvm_mock.svh"))
        .filter(|f| CONFIG.get().is_none_or(|c| !c.hides_file(f)))
        .cloned()
        .collect()
}

/// `--init`: discovers the project, learns its naming, writes
/// `.uvmcheck/config.py`, then lints with it to write the first baseline
/// (`.uvmcheck/baseline.json`, or `--update-baseline FILE`).
fn init_project(args: &[String]) -> ! {
    let fail = |e: String| -> ! {
        eprintln!("error: {e}");
        std::process::exit(2);
    };
    for existing in [".uvmcheck/config.py", ".uvmcheck/config.json", "uvmcheck_config.py"] {
        if std::path::Path::new(existing).exists() {
            fail(format!("{existing} already exists; remove it to run --init again"));
        }
    }
    // Sources given on the command line are the sources; other options stay.
    let mut cli_files = Vec::new();
    let mut prev = String::new();
    for a in args {
        let takes_value = matches!(prev.as_str(), "-f" | "-F" | "-v" | "-y" | "-top" | "--top" | "--rule" | "--preset");
        if !takes_value && !a.starts_with(['-', '+']) && std::path::Path::new(a).is_file() {
            cli_files.push(std::path::PathBuf::from(a));
        }
        prev = a.clone();
    }
    let d = dv_lint::init::discover(std::path::Path::new("."), &cli_files);
    if cli_files.is_empty() && d.sources_rel.is_empty() && d.filelists_rel.is_empty() {
        fail(format!("no SystemVerilog sources found under {}", d.root.display()));
    }
    println!(
        "[dv-lint] init: {} SystemVerilog files; {}; {}",
        d.sv_files,
        if d.filelists_rel.is_empty() {
            format!("{} top-level sources", d.sources_rel.len())
        } else {
            format!("{} file list(s)", d.filelists_rel.len())
        },
        d.uvm_note
    );
    let (design, _trees) = sv_sema::load_with_trees(&d.compile_args()).unwrap_or_else(|e| fail(e.to_string()));
    let library = d.library_rel.first().map(|l| {
        let e = match l.strip_prefix("$UVM_HOME") {
            Some(rest) => format!("{}{rest}", std::env::var("UVM_HOME").unwrap_or_default()),
            None => l.clone(),
        };
        let p = d.root.join(e);
        p.canonicalize().unwrap_or(p)
    });
    let naming = dv_lint::init::infer_naming(&design, library.as_deref());
    if let Err(e) = std::fs::create_dir_all(".uvmcheck")
        .and_then(|()| std::fs::write(".uvmcheck/config.py", dv_lint::init::render_config(&d, &naming)))
    {
        fail(format!("cannot write .uvmcheck/config.py: {e}"));
    }
    // Lint with the new config (which names the sources) for the baseline.
    let given: Vec<String> = std::env::args().skip(1).collect();
    let mut rerun: Vec<String> = Vec::new();
    let mut baseline = ".uvmcheck/baseline.json".to_string();
    let mut it = given.iter();
    while let Some(a) = it.next() {
        if a == "--init" || cli_files.iter().any(|f| f.as_os_str() == a.as_str()) {
            continue;
        }
        if a == "--update-baseline" {
            baseline = it.next().cloned().unwrap_or(baseline);
            continue;
        }
        if let Some(b) = a.strip_prefix("--update-baseline=") {
            baseline = b.to_string();
            continue;
        }
        rerun.push(a.clone());
    }
    rerun.extend(["--no-cache".to_string(), "--update-baseline".to_string(), baseline.clone()]);
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("dv-lint"));
    let out = std::process::Command::new(exe)
        .args(&rerun)
        .stderr(std::process::Stdio::inherit())
        .output()
        .unwrap_or_else(|e| fail(format!("cannot lint with the new config: {e}")));
    if out.status.code() == Some(2) {
        std::process::exit(2);
    }
    print!("\n[dv-lint] Wrote .uvmcheck/config.py");
    if naming.is_empty() {
        print!(" (the default naming conventions already fit this code)");
    }
    println!();
    for n in &naming {
        let what = if n.disable { "disabled".to_string() } else { format!("pattern {}", n.pattern) };
        println!("  naming {}: {what} ({})", n.target, n.evidence);
    }
    println!(
        "[dv-lint] Existing findings are in the baseline {baseline}; from now on only new findings are reported.\nReview the config, then run `uvmcheck` from this folder."
    );
    std::process::exit(0);
}

/// The project configuration in use, if any.
static LIBRARY: std::sync::OnceLock<dv_lint::library::Library> = std::sync::OnceLock::new();
static REPORT_LIBRARY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
    design: Option<&Design>,
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
    let (bad_waivers, rest): (Vec<_>, Vec<_>) = found.into_iter().partition(dv_lint::is_bad_waiver_pattern);
    found = rest;
    if !REPORT_LIBRARY.load(std::sync::atomic::Ordering::Relaxed) {
        let none = dv_lint::library::Library::default();
        let lib = LIBRARY.get().unwrap_or(&none);
        let hidden = lib.hide(&mut found, files, CONFIG.get().is_some_and(|c| c.reports_library_macros()));
        if hidden > 0 {
            let roots: Vec<String> = lib.roots().iter().map(|r| r.display().to_string()).collect();
            eprintln!(
                "[dv-lint] {hidden} finding(s) in the UVM library ({}) are not shown; --report-library shows them",
                roots.join(", ")
            );
        }
    }
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
    if let Some(s) = SUPPRESS.get() {
        let regraded = s.regraded();
        if regraded > 0 {
            eprintln!("[dv-lint] {regraded} finding(s) re-graded by waivers with a severity");
        }
        let stale = s.stale_baseline();
        if stale > 0 && UPDATE_BASELINE.get().is_none() {
            eprintln!(
                "[dv-lint] {stale} baseline entr{} matched no finding (fixed, or the flagged line changed); run with --update-baseline to drop them",
                if stale == 1 { "y" } else { "ies" }
            );
        }
    }
    // --changed-since and the waiver hygiene reports.
    let mut files_owned = files.to_vec();
    let mut outcome = dv_lint::cli::Outcome::default();
    if let Some(x) = EXTRA.get() {
        let active: std::collections::HashSet<&str> =
            ACTIVE.get().map(|a| a.iter().copied().collect()).unwrap_or_default();
        let user = USER_FILES.get().map_or(&[][..], |u| &u[..]);
        let root = ROOT.get().cloned().unwrap_or_default();
        outcome = x
            .after_suppress(&mut found, &mut files_owned, &root, SUPPRESS.get(), CONFIG.get(), user, &active)
            .unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(2);
            });
    }
    found.extend(bad_waivers);
    let files = &files_owned[..];
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
    if let (true, Some(design)) = (time, design) {
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
    // A report written to standard output (`--json -`, `--sarif -`) takes
    // the place of the text report, so the output parses as that report.
    let to_stdout = |o: Option<&String>| o.is_some_and(|p| p == "-");
    let text_report = !to_stdout(JSON_OUT.get()) && !to_stdout(SARIF_OUT.get());
    for f in found.iter().filter(|_| text_report) {
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
        if path == "-" {
            print!("{}", dv_lint::report::sarif(files, found));
        } else if let Err(e) = std::fs::write(path, dv_lint::report::sarif(files, found)) {
            eprintln!("error: cannot write {path}: {e}");
            std::process::exit(2);
        }
    }
    if let Some(path) = JSON_OUT.get() {
        if path == "-" {
            print!("{}", json_report(files, found, waived));
        } else if let Err(e) = std::fs::write(path, json_report(files, found, waived)) {
            eprintln!("error: cannot write {path}: {e}");
            std::process::exit(2);
        }
    }
    if let (Some(path), Some(m)) = (EXTRA.get().and_then(|x| x.map.as_ref()), MAP.get()) {
        match m.write(path, files, found) {
            Ok(()) => eprintln!("[dv-lint] Testbench map written to: {path}"),
            Err(e) => {
                eprintln!("error: cannot write {path}: {e}");
                std::process::exit(2);
            }
        }
    }
    // As uvmcheck: warnings and infos alone do not fail the run (but a
    // failed --strict-new-files does; a failed --strict-waivers exits 4).
    let strict = EXTRA.get().is_some_and(|x| x.strict_waivers);
    std::process::exit(dv_lint::cli::exit_status(found, outcome, strict));
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
