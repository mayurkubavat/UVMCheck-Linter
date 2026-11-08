//! `dv-lint --json FILE` writes uvmcheck's JSON report form.

#[test]
fn json_report_has_uvmchecks_fields() {
    let dir = std::env::temp_dir().join(format!("dv-lint-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.sv");
    std::fs::write(&src, "module m; logic [7:0] a; initial a = 255; // TODO: \"quoted\"\nendmodule\n").unwrap();
    let out = dir.join("r.json");
    let st =
        std::process::Command::new(env!("CARGO_BIN_EXE_dv-lint")).arg("--json").arg(&out).arg(&src).status().unwrap();
    assert_eq!(st.code(), Some(1), "findings exit 1");
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
    assert!(run(&[]), "the default selection has style rules");
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
