//! `dv-lint --json FILE` writes uvmcheck's JSON report form.

#[test]
fn json_report_has_uvmchecks_fields() {
    let dir = std::env::temp_dir().join(format!("dv-lint-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.sv");
    std::fs::write(&src, "module m; logic [7:0] a; initial a = 255; // TODO: \"quoted\"\nendmodule\n").unwrap();
    let out = dir.join("r.json");
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .args(["--preset", "style", "--json"])
        .arg(&out)
        .arg(&src)
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(0), "only infos and warnings: exit 0, as uvmcheck");
    let text = std::fs::read_to_string(&out).unwrap();
    for key in ["\"tool\": \"dv-lint\"", "\"compileErrors\": 0", "\"violations\": [", "\"ruleId\": \"SV-STY-003\""] {
        assert!(text.contains(key), "{key} in {text}");
    }
    // The TODO comment's quotes are escaped.
    assert!(text.contains("\\\"quoted\\\""), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn presets_select_rules_as_uvmcheck() {
    let dir = std::env::temp_dir().join(format!("dv-lint-preset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.sv");
    // SV-STY-003 (a TODO comment) is a style rule, not a soundness one.
    std::fs::write(&src, "module m; // TODO: later\nendmodule\n").unwrap();
    let run = |extra: &[&str]| {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint")).args(extra).arg(&src).output().unwrap();
        String::from_utf8_lossy(&o.stdout).contains("SV-STY-003")
    };
    assert!(!run(&[]), "the default selection has no style rules");
    assert!(!run(&["--preset", "default"]));
    assert!(run(&["--preset", "style"]));
    assert!(run(&["--preset", "all"]));
    assert!(!run(&["--preset", "soundness"]));
    assert!(!run(&["--preset=soundness"]));
    assert!(run(&["--preset", "soundness", "--rule", "SV-STY-003"]), "--rule wins");
    let bad = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .args(["--preset", "nope"])
        .arg(&src)
        .status()
        .unwrap();
    assert_eq!(bad.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn project_config_paths_rules_and_waivers() {
    let dir = std::env::temp_dir().join(format!("dv-lint-config-{}", std::process::id()));
    for d in [".uvmcheck", "vip", "tb"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(
        dir.join(".uvmcheck/config.json"),
        r#"{"preset": "all", "library_paths": ["vip/**"], "rules": {"SV-STY-003": "off", "SV-BAN-001": {"severity": "ERROR"}},
            "waivers": [{"rule": "SV-STY-010", "path": "tb/top.sv", "line": 3, "reason": "known"}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("tb/top.sv"),
        "module top; // TODO x\nreg a; integer i;\ninitial i = 5;\ninitial i = 6;\nendmodule\n",
    )
    .unwrap();
    std::fs::write(dir.join("vip/v.sv"), "module v; reg c; endmodule\n").unwrap();
    let run = |extra: &[&str]| {
        let out = dir.join("r.json");
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(extra)
            .arg("--json")
            .arg(&out)
            .args(["tb/top.sv", "vip/v.sv"])
            .output()
            .unwrap();
        std::fs::read_to_string(&out).unwrap()
    };
    let with = run(&[]);
    assert!(!with.contains("SV-STY-003"), "rule turned off: {with}");
    assert!(!with.contains("v.sv"), "library code is not reported: {with}");
    assert!(
        with.contains(
            "\"ruleId\": \"SV-BAN-001\", \"ruleName\": \"LegacyRegWireInTestbench\", \"severity\": \"ERROR\""
        ),
        "{with}"
    );
    assert!(with.contains("\"waived\": 1"), "{with}");
    assert!(
        with.contains("\"line\": 4, \"column\": 13, \"message\": \"Unsized decimal literal '6'"),
        "not waived: {with}"
    );
    let without = run(&["--no-config", "--preset", "all"]);
    assert!(without.contains("SV-STY-003") && without.contains("v.sv"), "{without}");
    // Without the config's preset the default rule set runs: no style rules.
    let default = run(&["--no-config"]);
    assert!(!default.contains("SV-STY-003") && default.contains("SV-BAN-001"), "{default}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rule_options_from_the_config() {
    let dir = std::env::temp_dir().join(format!("dv-lint-options-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("uvmcheck.json"),
        r#"{"rules": {"MaxLineLength": {"options": {"max_columns": 40}}, "SV-MET-003": {"options": {"max_args": 2}}}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("a.sv"),
        "module m;\nfunction automatic int f(int a, int b, int c); return a + b + c; endfunction // a long line here\nendmodule\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint")).current_dir(&dir).arg("a.sv").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("'f' takes 3 arguments (limit 2)"), "{text}");
    assert!(text.contains("threshold of 40 characters"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn style_block_from_the_config() {
    let dir = std::env::temp_dir().join(format!("dv-lint-style-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("uvmcheck.json"),
        r#"{"style": {"max_line_length": 30, "allow_tabs": true, "mandatory_matching_end_label": false,
            "require_file_header": true, "file_header_pattern": "Copyright|SPDX-License-Identifier",
            "file_matches_class_name": true}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("bar.sv"),
        "class foo;\n\tint a;\n  function void f(); endfunction // padding padding\nendclass\n",
    )
    .unwrap();
    std::fs::write(dir.join("ok.sv"), "// SPDX-License-Identifier: MIT\nclass ok; endclass\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["bar.sv", "ok.sv"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let has = |id: &str, file: &str| text.lines().any(|l| l.contains(id) && l.contains(file));
    assert!(has("SV-STY-103", "bar.sv") && text.contains("limit of 30 characters"), "{text}");
    assert!(!text.contains("SV-STY-101"), "tabs allowed: {text}");
    assert!(!text.contains("SV-STY-104"), "end labels not required: {text}");
    assert!(has("SV-STY-105", "bar.sv") && !has("SV-STY-105", "ok.sv"), "{text}");
    assert!(has("SV-STY-106", "bar.sv") && !has("SV-STY-106", "ok.sv"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn naming_block_from_the_config() {
    let dir = std::env::temp_dir().join(format!("dv-lint-naming-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("uvmcheck.json"),
        r#"{"naming": {"Class": {"enabled": true}, "Typedef": {"suffix": "_type", "severity": "ERROR"},
            "Package": {"suffix": ""}, "Coverpoint": {"pattern": "^x_.*$"}}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("t.sv"), "package my_stuff; endpackage\nclass myThing; endclass\ntypedef int count;\n")
        .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--rule", "SV-NAM-100", "t.sv"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("Class identifier 'myThing' does not satisfy naming convention: must follow PascalCase."),
        "{text}"
    );
    assert!(text.contains("[ERROR] SV-NAM-111") && text.contains("must end with suffix '_type'"), "{text}");
    assert!(
        text.contains("Package identifier 'my_stuff' does not satisfy naming convention: must match pattern"),
        "suffix dropped, pattern kept: {text}"
    );
    assert!(!text.contains("must end with suffix '_pkg'"), "{text}");
    assert!(!err.contains("naming.Coverpoint"), "a regex pattern is applied: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn waiver_files_regrades_and_baselines() {
    let dir = std::env::temp_dir().join(format!("dv-lint-baseline-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".uvmcheck")).unwrap();
    std::fs::write(
        dir.join(".uvmcheck/config.json"),
        r#"{"preset": "all", "waivers": [{"rule": "SV-BAN-001", "severity": "error", "reason": "regrade"}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("w.json"),
        r#"{"waivers": [{"rule": "SV-STY-010", "file": "*t.sv*", "line": 3, "reason": "known"}]}"#,
    )
    .unwrap();
    let src = dir.join("t.sv");
    std::fs::write(&src, "module m;\nreg a; integer i;\ninitial i = 5;\ninitial i = 6;\nendmodule\n").unwrap();
    let run = |extra: &[&str]| {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(extra)
            .arg("t.sv")
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    let with = run(&["--waivers", "w.json"]);
    assert!(with.contains("[ERROR] SV-BAN-001") && with.contains("[re-graded by waiver #1 (SV-BAN-001)]"), "{with}");
    assert!(!with.contains("Unsized decimal literal '5'") && with.contains("Unsized decimal literal '6'"), "{with}");
    // A baseline of everything, then a line moves: still matched.
    run(&["--update-baseline", "b.json"]);
    let moved = std::fs::read_to_string(&src).unwrap();
    std::fs::write(&src, format!("// moved\n{moved}module n; reg z; endmodule\n")).unwrap();
    let after = run(&["--baseline", "b.json"]);
    let reported: Vec<&str> = after.lines().filter(|l| l.contains(": [")).collect();
    assert!(reported.iter().all(|l| l.contains("t.sv:7:")), "only the new module's findings: {after}");
    assert!(!reported.is_empty(), "{after}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sarif_report() {
    let dir = std::env::temp_dir().join(format!("dv-lint-sarif-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.sv"), "module m; reg a; endmodule\n").unwrap();
    let out = dir.join("r.sarif");
    std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--no-config", "--sarif"])
        .arg(&out)
        .arg("t.sv")
        .output()
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for key in [
        "\"version\": \"2.1.0\"",
        "\"name\": \"dv-lint\"",
        "\"id\": \"SV-BAN-001\"",
        "\"ruleId\": \"SV-BAN-001\",\n          \"level\": \"note\"",
        "\"remediationMinutes\": 5",
    ] {
        assert!(text.contains(key), "{key} in {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn html_dashboard() {
    let dir = std::env::temp_dir().join(format!("dv-lint-html-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.sv"), "module m; reg a; endmodule\n").unwrap();
    let out = dir.join("r.html");
    std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--no-config", "--html"])
        .arg(&out)
        .arg("t.sv")
        .output()
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("<!DOCTYPE html>") && text.trim_end().ends_with("</html>"), "{text}");
    assert!(text.contains("<span class=\"rule-id\">SV-BAN-001</span>"), "{text}");
    assert!(text.contains("module m; reg a; endmodule</div>"), "the source line: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inline_waiver_patterns_are_regexes() {
    // Line 2: "literal .9." does not match the message about '5', so it stays
    // (as with uvmcheck's std::regex). Line 3: a regex that matches waives.
    // Line 4: a pattern dv-lint refuses (a backreference) waives nothing and
    // is a UVM-WV-001 error at the comment, whatever --rule selects.
    let dir = std::env::temp_dir().join(format!("dv-lint-inline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.sv"),
        "module m; integer i;\ninitial i = 5; // uvmcheck:waive SV-STY-010 \"literal .9.\"\n\
         initial i = 6; // uvmcheck:waive SV-STY-010 \"literal '[0-9]'\"\n\
         initial i = 7; // uvmcheck:waive SV-STY-010 \"(7)\\\\1\"\nendmodule\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--no-config", "--no-cache", "--rule", "SV-STY-010", "t.sv"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let sty: Vec<&str> = text.lines().filter(|l| l.contains("] SV-STY-010 (")).collect();
    assert_eq!(sty.len(), 2, "{text}");
    assert!(sty[0].contains("t.sv:2:") && sty[1].contains("t.sv:4:"), "{text}");
    let wv: Vec<&str> = text.lines().filter(|l| l.contains("UVM-WV-001")).collect();
    assert_eq!(wv.len(), 1, "{text}");
    assert!(wv[0].contains("t.sv:4:1:") && wv[0].contains("invalid message regex '(7)\\1'"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn waiver_file_with_a_bad_pattern_stops_the_run() {
    let dir = std::env::temp_dir().join(format!("dv-lint-waiver-re-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.sv"), "module m; integer i; initial i = 5; endmodule\n").unwrap();
    std::fs::write(dir.join("w.json"), r#"[{"rule": "SV-STY-010", "message": "(?=x)", "reason": "r"}]"#).unwrap();
    std::fs::write(dir.join("ok.json"), r#"[{"rule": "SV-STY-010", "message": "literal '[5]'", "reason": "r"}]"#)
        .unwrap();
    let run = |w: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(["--no-config", "--no-cache", "--rule", "SV-STY-010", "--waivers", w, "t.sv"])
            .output()
            .unwrap()
    };
    let bad = run("w.json");
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("invalid message regex '(?=x)'"));
    let ok = run("ok.json");
    assert!(!String::from_utf8_lossy(&ok.stdout).contains("SV-STY-010"), "{}", String::from_utf8_lossy(&ok.stdout));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_rewrites_and_dry_run_shows_the_diff() {
    let dir = std::env::temp_dir().join(format!("dv-lint-fix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("a.sv");
    std::fs::write(&src, "module m;   \n  logic a;\t\nendmodule  \n").unwrap();
    let run = |extra: &[&str]| {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(["--no-config", "--fix-rule", "SV-STY-015"])
            .args(extra)
            .arg("a.sv")
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    let dry = run(&["--fix-dry-run"]);
    assert!(dry.starts_with("--- a/a.sv\n+++ b/a.sv\n@@ -1,3 +1,3 @@\n-module m;   \n+module m;\n"), "{dry}");
    assert!(dry.contains("[uvmcheck] Would apply 3 fix-it(s) in 1 file(s)."), "{dry}");
    assert_eq!(
        std::fs::read_to_string(&src).unwrap(),
        "module m;   \n  logic a;\t\nendmodule  \n",
        "dry run writes nothing"
    );
    let fixed = run(&["--fix"]);
    assert!(fixed.contains("[uvmcheck] Applied 3 fix-it(s) in 1 file(s)."), "{fixed}");
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "module m;\n  logic a;\nendmodule\n");
    assert!(!fixed.contains("SV-STY-015"), "the next pass reports on the fixed text: {fixed}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_skips_files_with_semantic_errors() {
    // A `solve before` on a non-random property: slang rejects the file and
    // uvmcheck applies no fix in it; the native front end now agrees.
    let dir = std::env::temp_dir().join(format!("dv-lint-fix-cmp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = "class c;   \n  rand int w; int plain;\n  constraint k_c { solve plain before w; }\nendclass\n";
    std::fs::write(dir.join("a.sv"), text).unwrap();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--no-config", "--no-cache", "--fix-rule", "SV-STY-015", "--fix", "a.sv"])
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("1 in files with compile errors not applied"), "{out}");
    assert_eq!(std::fs::read_to_string(dir.join("a.sv")).unwrap(), text, "the file is left as it was");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inputs_that_cannot_be_read_exit_2() {
    // As uvmcheck: the run cannot start (docs/guide/uvmcheck.md, Exit status).
    let dir = std::env::temp_dir().join(format!("dv-lint-exit2-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("adir")).unwrap();
    std::fs::write(dir.join("ok.sv"), "module m; endmodule\n").unwrap();
    std::fs::write(dir.join("nested.f"), "-f gone.f\n").unwrap();
    std::fs::write(dir.join("self.f"), "-f self.f\n").unwrap();
    let code = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(["--no-config", "--no-cache"])
            .args(args)
            .output()
            .unwrap()
            .status
            .code()
    };
    for args in [
        &["missing.sv"][..],
        &["ok.sv", "adir"],
        &["ok.sv", "-f", "nested.f"],
        &["ok.sv", "-f", "self.f"],
        &["ok.sv", "-v", "missing_lib.v"],
        &["ok.sv", "-y", "no_dir"],
        &["--no-such-option", "ok.sv"],
    ] {
        assert_eq!(code(args), Some(2), "{args:?}");
    }
    assert_eq!(code(&["ok.sv", "-I", "no_dir"]), Some(0), "a missing include directory is not an error");
    assert_eq!(code(&["ok.sv", "+acc", "-l", "run.log"]), Some(0), "simulator options are ignored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redundant_boolean_fix_keeps_x_and_z_behaviour() {
    // SV-RDB-005: `b == 1'b0` -> `!b` everywhere (both X for X and Z);
    // `b == 1'b1` -> `b` only in a condition or on a 2-state `b`.
    let dir = std::env::temp_dir().join(format!("dv-lint-fix-rdb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = "module m(input logic a, input bit b, output logic y, output logic z, output bit q);\n\
               always_comb begin\n\
               if (a == 1'b1) y = 1'b0;\n\
               z = (a == 1'b0);\n\
               z = (a == 1'b1);\n\
               q = (b == 1'b1);\n\
               end\n\
               endmodule\n";
    let run = |extra: &[&str]| {
        std::fs::write(dir.join("a.sv"), src).unwrap();
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args([
                "--no-config",
                "--no-cache",
                "--preset",
                "all",
                "--experimental",
                "--fix",
                "--fix-rule",
                "SV-RDB-005",
            ])
            .args(extra)
            .arg("a.sv")
            .output()
            .unwrap();
        std::fs::read_to_string(dir.join("a.sv")).unwrap()
    };
    let safe = run(&[]);
    assert!(safe.contains("if (a) y = 1'b0;"), "{safe}");
    assert!(safe.contains("z = (!a);"), "{safe}");
    assert!(safe.contains("z = (a == 1'b1);"), "a 4-state value outside a condition is left: {safe}");
    assert!(safe.contains("q = (b);"), "{safe}");
    let all = run(&["--unsafe-fixes"]);
    assert!(all.contains("z = (a);"), "{all}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn banner_fix_makes_78_columns() {
    // SV-STY-007 / SV-STY-008: a ragged separator comment becomes 78 characters.
    let dir = std::env::temp_dir().join(format!("dv-lint-fix-banner-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.sv"),
        "class c;\n  //----------\n  function void f(); endfunction\n  //======\nendclass\n",
    )
    .unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
        .current_dir(&dir)
        .args(["--no-config", "--no-cache", "--fix", "--fix-rule", "SV-STY-007,SV-STY-008", "a.sv"])
        .output()
        .unwrap();
    let text = std::fs::read_to_string(dir.join("a.sv")).unwrap();
    assert!(text.contains(&format!("  //{}\n", "-".repeat(76))), "{text}");
    assert!(text.contains(&format!("  //{}\n", "=".repeat(76))), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exit_status_follows_errors_only() {
    let dir = std::env::temp_dir().join(format!("dv-lint-exit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("w.sv"), "module m; // TODO: x\nendmodule\n").unwrap();
    // A syntax error is an ERROR finding (SV-CMP-001).
    std::fs::write(dir.join("e.sv"), "module m; logic a endmodule\n").unwrap();
    let code = |f: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(["--no-config", f])
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(code("w.sv"), Some(0));
    assert_eq!(code("e.sv"), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A rule the project config configures runs although the default rule set
/// leaves its profile out; `--preset` on the command line narrows the run
/// again, and `enabled: false` is not a request for the rule.
#[test]
fn a_configured_rule_runs_outside_the_default_set() {
    let dir = std::env::temp_dir().join(format!("dv-lint-configured-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".uvmcheck")).unwrap();
    // SV-STY-003 (a TODO comment) and SV-STY-015 (trailing whitespace) are style rules.
    std::fs::write(dir.join("t.sv"), "module m; // TODO: later  \nendmodule\n").unwrap();
    let run = |config: &str, extra: &[&str]| {
        std::fs::write(dir.join(".uvmcheck/config.json"), config).unwrap();
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint"))
            .current_dir(&dir)
            .args(["--no-cache"])
            .args(extra)
            .arg("t.sv")
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)
    };
    let plain = run("{}", &[]);
    assert!(!plain.contains("SV-STY-003") && !plain.contains("SV-STY-015"), "{plain}");
    let configured = run(r#"{"rules": {"SV-STY-003": {"severity": "ERROR"}, "TrailingWhitespace": false}}"#, &[]);
    assert!(configured.contains("[ERROR] SV-STY-003"), "{configured}");
    assert!(!configured.contains("SV-STY-015"), "turned off is not asked for: {configured}");
    let narrowed = run(r#"{"rules": {"SV-STY-003": {"severity": "ERROR"}}}"#, &["--preset", "soundness"]);
    assert!(!narrowed.contains("SV-STY-003"), "{narrowed}");
    // A style block asks for the style engine (SV-STY-100, reporting as SV-STY-102).
    let block = run(r#"{"style": {"allow_trailing_whitespace": false}}"#, &[]);
    assert!(block.contains("SV-STY-102"), "{block}");
    // The project's preset stands in for --preset.
    let preset = run(r#"{"preset": "style"}"#, &[]);
    assert!(preset.contains("SV-STY-003") && preset.contains("SV-STY-015"), "{preset}");
    let bad = run(r#"{"preset": "nope"}"#, &[]);
    assert!(bad.contains("preset: unknown preset 'nope'"), "{bad}");
    let _ = std::fs::remove_dir_all(&dir);
}
