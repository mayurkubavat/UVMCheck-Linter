//! The uvmcheck options dv-lint ported last: --explain, --changed-since,
//! the waiver-hygiene reports, the result cache, --map, --init and
//! --python-rules (cpp/tests/test_*.py check the same against uvmcheck).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dv-lint-rest-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dv-lint")).args(args).current_dir(cwd).output().unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
}

const TB: &str =
    "module tb;\n  int a, b;\n  initial begin\n    if (a == a) $display(\"old\");\n  end\nendmodule : tb\n";

#[test]
fn explain_prints_a_rule() {
    let d = tmp("explain");
    let o = run(&d, &["--explain", "SV-DUP-002"]);
    assert_eq!(o.status.code(), Some(0));
    let out = text(&o);
    assert!(out.contains("CloneFragments") && out.contains("MG-7.1") && out.contains("min_tokens = 50"), "{out}");
    let o = run(&d, &["--explain=NO-SUCH-001"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o).contains("unknown rule"));
}

#[test]
fn changed_since_keeps_changed_lines_and_untracked_files() {
    let d = tmp("changed");
    std::fs::write(d.join("tb.sv"), TB).unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["add", "tb.sv"]);
    git(&d, &["commit", "-q", "-m", "base"]);
    std::fs::write(d.join("tb.sv"), TB.replace("(\"old\");\n", "(\"old\");\n    if (b == b) $display(\"new\");\n"))
        .unwrap();
    std::fs::write(d.join("extra.sv"), TB.replace("tb", "extra")).unwrap();
    let lines = |o: &Output| -> Vec<String> {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| l.contains("SV-COR-008"))
            .map(|l| {
                let mut p = l.split(':');
                let f = Path::new(p.next().unwrap()).file_name().unwrap().to_string_lossy().into_owned();
                format!("{f}:{}", p.next().unwrap())
            })
            .collect()
    };
    let all = run(&d, &["--no-config", "--rule", "SV-COR-008", "tb.sv", "extra.sv"]);
    assert_eq!(lines(&all), ["extra.sv:4", "tb.sv:4", "tb.sv:5"]);
    let o = run(&d, &["--no-config", "--rule", "SV-COR-008", "tb.sv", "extra.sv", "--changed-since", "HEAD"]);
    assert_eq!(lines(&o), ["extra.sv:4", "tb.sv:5"]);
    assert!(text(&o).contains("1 elsewhere not shown"), "{}", text(&o));
    // SV-COR-008 is an ERROR: exit 1 anyway; a new file fails the strict gate
    // only through its finding count, which the message names.
    let o =
        run(&d, &["--no-config", "--rule", "SV-COR-008", "extra.sv", "--changed-since", "HEAD", "--strict-new-files"]);
    assert!(text(&o).contains("--strict-new-files: 1 finding(s)"), "{}", text(&o));
    let o = run(&d, &["--no-config", "tb.sv", "--changed-since", "no-such-ref"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o).contains("unknown git revision"));
    let o = run(&d, &["--no-config", "tb.sv", "--strict-new-files"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o).contains("requires --changed-since"));
}

#[test]
fn waiver_hygiene_reports_and_strict_exit() {
    let d = tmp("hygiene");
    std::fs::create_dir_all(d.join(".uvmcheck")).unwrap();
    // Line 4 is waived inline; the comment on line 2 names SV-COR-008 and waives nothing.
    std::fs::write(
        d.join("tb.sv"),
        "module tb;\n  int a; // uvmcheck:waive SV-COR-008 -- stale\n  initial begin\n    if (a == a) $display(\"x\"); // uvmcheck:waive SV-COR-008 -- known\n  end\nendmodule : tb\n",
    )
    .unwrap();
    std::fs::write(
        d.join(".uvmcheck/waivers.json"),
        r#"{"waivers": [{"rule": "SV-COR-001", "path": "tb.sv", "reason": "x", "owner": "dv"},
                        {"rule": "SV-NOPE-001", "reason": "never matches"}]}"#,
    )
    .unwrap();
    let o = run(&d, &["--rule", "SV-COR-008", "--rule", "SV-COR-001", "tb.sv", "--waiver-report"]);
    let out = text(&o);
    assert!(out.contains("=== Waiver hygiene ==="), "{out}");
    assert!(out.contains("Waivers: 2 written, 1 matched nothing, 0 expired, 1 without an owner"), "{out}");
    assert!(out.contains("unused:  waivers.json #2 (SV-NOPE-001)"), "{out}");
    assert!(out.contains("Waiver comments: 2 in source, 1 waive nothing"), "{out}");
    assert!(out.contains("tb.sv:2: // uvmcheck:waive SV-COR-008 -- stale"), "{out}");
    assert!(out.contains("Waived findings by rule: SV-COR-001 1, SV-COR-008 1"), "{out}");
    assert!(out.contains("Waived findings by owner: (inline comment) 1, dv 1"), "{out}");
    assert_eq!(o.status.code(), Some(0), "the report alone does not fail");
    let o = run(&d, &["--rule", "SV-COR-008", "--rule", "SV-COR-001", "tb.sv", "--report-unused-waivers"]);
    let out = text(&o);
    assert!(
        out.contains("UVM-WV-001 (UnusedWaiver): waiver waivers.json #2 (SV-NOPE-001) matched no diagnostic"),
        "{out}"
    );
    assert!(
        out.contains("tb.sv:2:1: [WARNING] UVM-WV-001 (UnusedWaiver): waiver comment matched no diagnostic"),
        "{out}"
    );
    let o = run(&d, &["--rule", "SV-COR-008", "--rule", "SV-COR-001", "tb.sv", "--strict-waivers"]);
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
}

#[test]
fn the_cache_replays_until_a_source_changes() {
    let d = tmp("cache");
    std::fs::write(d.join("tb.sv"), TB).unwrap();
    let args = ["--no-config", "--rule", "SV-COR-008", "tb.sv", "--json"];
    let first = run(&d, &[&args[..], &["a.json"]].concat());
    assert!(text(&first).contains("cache: stored 1 compilation unit"), "{}", text(&first));
    assert!(d.join(".uvmcheck_cache/native/manifest.json").is_file());
    // Another report file: the same lint, served from the cache.
    let second = run(&d, &[&args[..], &["b.json"]].concat());
    assert!(text(&second).contains("served from cache"), "{}", text(&second));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.status.code(), second.status.code());
    let strip = |f: &str| std::fs::read_to_string(d.join(f)).unwrap();
    assert_eq!(strip("a.json"), strip("b.json"));
    std::fs::write(d.join("tb.sv"), TB.replace("a == a", "a == b")).unwrap();
    let edited = run(&d, &[&args[..], &["c.json"]].concat());
    let out = text(&edited);
    assert!(out.contains("cache miss: tb.sv changed") && !out.contains("served from cache"), "{out}");
    assert!(!String::from_utf8_lossy(&edited.stdout).contains("SV-COR-008"));
    // Another rule selection: another key (and the one entry is replaced).
    let other = run(&d, &["--no-config", "--rule", "SV-COR-001", "tb.sv"]);
    assert!(!text(&other).contains("served from cache"));
    // --no-cache neither reads nor writes; --clear-cache starts afresh.
    let fresh = run(&d, &[&args[..], &["d.json", "--no-cache"]].concat());
    assert!(!text(&fresh).contains("cache"), "{}", text(&fresh));
    std::fs::write(d.join(".uvmcheck_cache/native/manifest.json"), "{ not json").unwrap();
    let corrupt = run(&d, &[&args[..], &["e.json"]].concat());
    assert!(text(&corrupt).contains("cache: stored"), "a corrupt cache is a full run: {}", text(&corrupt));
    let cleared = run(&d, &[&args[..], &["f.json", "--clear-cache", "--cache-dir", "elsewhere"]].concat());
    assert!(text(&cleared).contains("cache: stored"));
    assert!(d.join("elsewhere/native/manifest.json").is_file());
}

#[test]
fn map_writes_the_testbench_as_json_and_html() {
    let d = tmp("map");
    let r = repo();
    let tb = r.join("testdata/map/map_tb.sv").display().to_string();
    let inc = r.join("testdata/common").display().to_string();
    let o = run(&d, &["--no-config", &tb, "-I", &inc, "--map", "m.json"]);
    assert!(text(&o).contains("Testbench map written to: m.json"), "{}", text(&o));
    let j = std::fs::read_to_string(d.join("m.json")).unwrap();
    assert!(j.contains("\"roots\": [\"my_test\", \"BaseDriver\", \"MyDriver\"]"), "{j}");
    assert!(j.contains("{\"name\": \"dbg_ap\", \"type\": \"uvm_analysis_port#(pkt)\", \"connected\": false}"), "{j}");
    assert!(j.contains("{\"in\": \"my_env\", \"from\": \"agt.mon.ap\", \"to\": \"scb.imp\"}"), "{j}");
    run(&d, &["--no-config", &tb, "-I", &inc, "--map", "m.html"]);
    let h = std::fs::read_to_string(d.join("m.html")).unwrap();
    assert!(h.contains("<h1>Testbench map</h1>") && h.contains("uvm_test_top"), "{h}");
}

#[test]
fn init_learns_naming_and_writes_config_and_baseline() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let d = tmp("init");
    let agents = d.join("tb/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::copy(repo().join("testdata/common/uvm_mock.svh"), d.join("tb/uvm_mock.svh")).unwrap();
    let mut pkg = vec!["`include \"uvm_mock.svh\"".to_string()];
    for a in ["axi", "apb", "spi"] {
        std::fs::write(
            agents.join(format!("{a}_bfm.svh")),
            format!(
                "class {a}_bfm extends uvm_driver;\n  `uvm_component_utils({a}_bfm)\n  function new(string name, uvm_component parent); super.new(name, parent); endfunction\nendclass : {a}_bfm\n"
            ),
        )
        .unwrap();
        pkg.push(format!("`include \"agents/{a}_bfm.svh\""));
    }
    std::fs::write(d.join("tb/tb_classes.sv"), pkg.join("\n") + "\n").unwrap();
    std::fs::write(d.join("tb/tb_top.sv"), "module tb_top;\n  int x;\n  initial x = x;\nendmodule : tb_top\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .arg("--init")
        .current_dir(&d)
        .env("UVMCHECK_PYTHON_SDK", repo().join("python"))
        .env_remove("UVM_HOME")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let cfg = std::fs::read_to_string(d.join(".uvmcheck/config.py")).unwrap();
    assert!(cfg.contains("\"tb/tb_classes.sv\"") && cfg.contains("\"tb/tb_top.sv\""), "{cfg}");
    assert!(cfg.contains("\"UvmDriver\": {\"pattern\": r\"^[A-Za-z0-9_]*(_bfm)$\""), "{cfg}");
    assert!(d.join(".uvmcheck/baseline.json").is_file(), "{}", text(&o));
    let base = std::fs::read_to_string(d.join(".uvmcheck/baseline.json")).unwrap();
    assert!(!base.contains("UVM-NAM-101"), "the learnt naming applies: {base}");
    // A plain run now reports nothing new.
    let o = Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&d)
        .env("UVMCHECK_PYTHON_SDK", repo().join("python"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).trim().is_empty(), "{}", text(&o));
    let again = run(&d, &["--init"]);
    assert_eq!(again.status.code(), Some(2));
    assert!(text(&again).contains("already exists"));
}

#[test]
fn python_rules_report_findings() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let d = tmp("python");
    std::fs::write(
        d.join("rules.py"),
        "from uvmcheck.rule import Rule, Severity\n\
         class NoDrv(Rule):\n    id = 'PY-DRV-001'\n    name = 'NoDrv'\n    default_severity = Severity.Error\n\
         \x20   def analyze(self, ctx):\n        for c in ctx.classes:\n            if c.role == 'driver':\n                self.report(ctx, f'driver {c.name} in {c.uvm_base}', at=c)\n",
    )
    .unwrap();
    let r = repo();
    let tb = r.join("testdata/map/map_tb.sv").display().to_string();
    let inc = r.join("testdata/common").display().to_string();
    let o = Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .args(["--no-config", "--no-cache", "--python-rules", "rules.py", "--rule", "PY-DRV-001", &tb, "-I", &inc])
        .current_dir(&d)
        .env("UVMCHECK_PYTHON_SDK", r.join("python"))
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("[ERROR] PY-DRV-001 (NoDrv): driver my_drv in uvm_driver"), "{}", text(&o));
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn the_uvmcheck_front_runs_these_options_natively() {
    let d = tmp("front");
    std::fs::write(d.join("tb.sv"), TB).unwrap();
    let front = repo().join("tools/front/uvmcheck");
    let go = |args: &[&str]| {
        Command::new("sh")
            .arg(&front)
            .args(args)
            .current_dir(&d)
            .env("UVMCHECK_NATIVE", env!("CARGO_BIN_EXE_dv-lint"))
            .env("UVMCHECK_SLANG", "/nonexistent/uvmcheck-slang")
            .env_remove("UVMCHECK_ENGINE")
            .output()
            .unwrap()
    };
    for args in [
        &["--explain", "SV-DUP-002"][..],
        &["--explain=SV-DUP-002"],
        &["--no-config", "--waiver-report", "--report-unused-waivers", "--strict-waivers", "tb.sv"],
        &["--no-config", "--cache-dir", "c", "--clear-cache", "--map", "m.json", "tb.sv"],
        &["--no-config", "--python-rules", "none.py", "--no-cache", "tb.sv"],
    ] {
        let o = go(args);
        assert!(!text(&o).contains("needs the Slang engine"), "{args:?}: {}", text(&o));
    }
}

#[test]
fn help_and_version_need_no_sources() {
    let d = tmp("help");
    let o = run(&d, &["--help"]);
    assert!(o.status.success(), "{}", text(&o));
    let help = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(help.contains("Usage: dv-lint [options]"), "{help}");
    assert!(help.contains("--preset NAME"), "{help}");
    assert_eq!(String::from_utf8_lossy(&run(&d, &["-h"]).stdout), help);
    let o = run(&d, &["--version"]);
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout), format!("dv-lint {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_mistyped_command_line_points_at_help() {
    let d = tmp("hint");
    for args in [&["--no-config"][..], &["--no-config", "--no-such-option", "x.sv"][..]] {
        let o = run(&d, args);
        assert_eq!(o.status.code(), Some(2), "{}", text(&o));
        assert!(text(&o).contains("run 'dv-lint --help' for the options"), "{}", text(&o));
    }
}

/// Every option `--help` lists is one the program takes: with it (and a
/// value where it wants one) the run must not end in "unknown command line
/// argument".
#[test]
fn every_option_in_help_is_accepted() {
    let d = tmp("help-options");
    std::fs::write(d.join("tb.sv"), TB).unwrap();
    std::fs::write(d.join("l.f"), "").unwrap();
    let mut seen = 0;
    for line in dv_lint::help::OPTIONS.lines() {
        // Option lines are indented by two spaces: `  --flag VALUE  text`,
        // `  -f FILE, -F FILE  text`.
        let Some(rest) = line.strip_prefix("  ") else { continue };
        if !rest.starts_with('-') && !rest.starts_with('+') {
            continue;
        }
        let spec = rest.split("  ").next().unwrap_or("");
        for form in spec.split(", ") {
            let mut words = form.split_whitespace();
            let Some(flag) = words.next() else { continue };
            let takes_value = words.next().is_some();
            // Options that end the run on their own, or change files.
            if ["-h", "--help", "--version", "--lsp", "--list", "--init", "--explain", "--fix", "--clear-cache"]
                .contains(&flag)
            {
                continue;
            }
            let flag = flag.split('[').next().unwrap_or(flag);
            let mut args: Vec<String> = vec!["--no-cache".into()];
            if flag.starts_with('+') {
                args.push(match flag {
                    "+incdir+DIR" => "+incdir+.".into(),
                    _ => "+define+A".into(),
                });
            } else {
                args.push(flag.to_string());
                if takes_value {
                    args.push(
                        match flag {
                            "-f" | "-F" => "l.f",
                            "-I" | "-y" | "--cache-dir" => ".",
                            "-v" => "tb.sv",
                            "-D" => "A",
                            "-top" => "tb",
                            "--preset" => "all",
                            "--rule" | "--fix-rule" => "SV-COR-008",
                            "--changed-since" => "HEAD",
                            "--python-rules" => "norules",
                            _ => "out.tmp",
                        }
                        .into(),
                    );
                }
            }
            args.push("tb.sv".into());
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let o = run(&d, &refs);
            assert!(!text(&o).contains("unknown command line argument"), "{flag}: {}", text(&o));
            seen += 1;
        }
    }
    assert!(seen > 30, "only {seen} options were found in the help text");
}

/// `--json -` and `--sarif -` write the report to standard output in place
/// of the text report, so a script can parse what the program prints.
#[test]
fn a_report_to_standard_output_replaces_the_text() {
    let d = tmp("stdout-report");
    std::fs::write(d.join("tb.sv"), TB).unwrap();
    let o = run(&d, &["--no-config", "--no-cache", "tb.sv", "--json", "-"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.trim_start().starts_with('{') && out.trim_end().ends_with('}'), "{out}");
    assert!(out.contains("\"ruleId\": \"SV-COR-008\""), "{out}");
    assert!(!out.contains("[ERROR]"), "{out}");
    assert!(!d.join("-").exists(), "a file named '-' was written");
    let o = run(&d, &["--no-config", "--no-cache", "tb.sv", "--sarif=-"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.trim_start().starts_with('{') && out.contains("\"version\": \"2.1.0\""), "{out}");
    assert!(!out.contains("[ERROR]"), "{out}");
}

/// `uvmcheck --lsp` hands the editor to the language server next to it.
#[test]
fn lsp_starts_the_language_server() {
    use std::io::{Read, Write};
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_uvmcheck"));
    // Built with the workspace; a run of this package alone may not have it.
    if !exe.with_file_name("dv-lsp").is_file() {
        return;
    }
    let mut child = std::process::Command::new(exe)
        .arg("--lsp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    let exit = r#"{"jsonrpc":"2.0","method":"exit"}"#;
    let mut stdin = child.stdin.take().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}Content-Length: {}\r\n\r\n{exit}", body.len(), exit.len()).unwrap();
    drop(stdin);
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let _ = child.wait();
    assert!(out.contains(r#""name":"dv-lsp""#) && out.contains("codeActionProvider"), "{out}");
}

/// A small UVM library (a package, a class and a macro with a self
/// comparison in its body) and a testbench that uses the macro.
fn library_and_testbench(d: &Path) {
    std::fs::create_dir_all(d.join("uvm/src/base")).unwrap();
    std::fs::write(
        d.join("uvm/src/uvm_pkg.sv"),
        "package uvm_pkg;\n`include \"base/uvm_object.svh\"\nendpackage : uvm_pkg\n",
    )
    .unwrap();
    std::fs::write(
        d.join("uvm/src/base/uvm_object.svh"),
        "class uvm_object;\n  function void lib();\n    int a;\n    if (a == a) $display(\"lib\");\n  endfunction : lib\nendclass : uvm_object\n",
    )
    .unwrap();
    std::fs::write(d.join("uvm/src/uvm_macros.svh"), "`define uvm_check(x) \\\n  if (x == x) $display(\"macro\");\n")
        .unwrap();
    std::fs::write(
        d.join("tb.sv"),
        "`include \"uvm_macros.svh\"\nmodule tb;\n  int a, b;\n  initial begin\n    `uvm_check(a)\n    if (b == b) $display(\"mine\");\n  end\nendmodule : tb\n",
    )
    .unwrap();
}

#[test]
fn a_finding_in_a_macro_names_the_macro_and_the_place_in_its_body() {
    let d = tmp("expansions");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("m.sv"),
        "`define SET_ALL(v) \\\n  begin \\\n    a = v; \\\n    if (a == a) b = v; \\\n  end\n\nmodule m;\n  int a, b;\n  initial begin\n    `SET_ALL(1)\n    if (b == b) a = 2;\n  end\nendmodule\n",
    )
    .unwrap();
    let out = text(&run(&d, &["--no-config", "--no-cache", "--rule", "SV-COR-008", "m.sv"]));
    let lines: Vec<&str> = out.lines().filter(|l| l.contains("m.sv:")).collect();
    // Reported where the macro is used, with the place in the body noted.
    assert_eq!(lines.len(), 3, "{out}");
    assert!(lines[0].contains("m.sv:10:0: [ERROR] SV-COR-008"), "{out}");
    assert!(lines[1].trim_start().starts_with("note: "), "{out}");
    assert!(lines[1].ends_with("m.sv:4:11: in the expansion of macro 'SET_ALL'"), "{out}");
    // Code written outside a macro has no such note.
    assert!(lines[2].contains("m.sv:11:11: [ERROR] SV-COR-008"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_uvm_library_is_left_out_beside_user_code() {
    let d = tmp("library");
    library_and_testbench(&d);
    let lint = |extra: &[&str]| {
        let mut args =
            vec!["--no-config", "--no-cache", "--rule", "SV-COR-008", "+incdir+uvm/src", "uvm/src/uvm_pkg.sv"];
        args.extend(extra);
        text(&run(&d, &args))
    };
    let lines = |out: &str| -> Vec<String> {
        out.lines().filter(|l| l.contains("SV-COR-008")).map(|l| l.split(": [").next().unwrap().to_string()).collect()
    };
    // Beside the testbench: only the user's own line, and a note.
    let out = lint(&["tb.sv"]);
    let found = lines(&out);
    assert_eq!(found.len(), 1, "{out}");
    assert!(found[0].ends_with("tb.sv:6:11"), "{out}");
    assert!(out.contains("2 finding(s) in the UVM library"), "{out}");
    assert!(out.contains("--report-library"), "{out}");
    // Asked for: the library's file and its macro's body too.
    let all = lint(&["tb.sv", "--report-library"]);
    assert_eq!(lines(&all).len(), 3, "{all}");
    assert!(!all.contains("in the UVM library"), "{all}");
    // The library alone is someone linting the library.
    let alone = lint(&[]);
    assert_eq!(lines(&alone).len(), 1, "{alone}");
    assert!(alone.contains("uvm_object.svh:4"), "{alone}");
    let _ = std::fs::remove_dir_all(&d);
}
