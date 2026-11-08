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
