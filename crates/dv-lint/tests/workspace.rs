//! A workspace reports what the command line reports for the project, and
//! reads the editor's text in place of the files it has open.

fn rules(c: &dv_lint::Checked) -> Vec<(&'static str, u32)> {
    c.findings.iter().map(|f| (f.rule, f.position_in(&c.files).1)).collect()
}

#[test]
fn a_workspace_applies_the_project_and_reads_editor_text() {
    let dir = std::env::temp_dir().join(format!("dv-lint-workspace-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".uvmcheck")).unwrap();
    std::fs::create_dir_all(dir.join("tb")).unwrap();
    // SV-STY-003 (a TODO comment, reported at the token after it) is a style
    // rule: off by default, on when the config configures it. The waiver
    // takes out the one reported on line 4.
    std::fs::write(
        dir.join(".uvmcheck/config.json"),
        r#"{"sources": ["tb/top.sv"], "rules": {"SV-STY-003": {"severity": "ERROR"}},
            "waivers": [{"rule": "SV-STY-003", "path": "tb/top.sv", "line": 4, "reason": "known"}]}"#,
    )
    .unwrap();
    let top = dir.join("tb/top.sv");
    std::fs::write(&top, "module top; // TODO first\nlogic a;\n// TODO waived\nendmodule\n").unwrap();
    let other = dir.join("tb/other.sv");
    std::fs::write(&other, "module other; // TODO elsewhere\nendmodule\n").unwrap();

    let ws = dv_lint::Workspace::open(&dir).expect("the project opens");
    assert!(ws.config_file().is_some_and(|f| f.ends_with(".uvmcheck/config.json")));
    let checked = ws.check(&[]).expect("the project checks");
    assert_eq!(rules(&checked), vec![("SV-STY-003", 2)]);
    assert_eq!(checked.findings[0].severity, dv_lint::Severity::Error);
    assert_eq!(checked.waived, 1);

    // A file the project does not compile is checked on its own when open.
    let checked = ws.check(std::slice::from_ref(&other)).unwrap();
    assert_eq!(checked.findings.len(), 2);

    // The editor's text stands in for the file: the first TODO moved down a
    // line, and the waived line now holds another.
    sv_syntax::overlay::set(&top, "module top;\n// TODO moved\nlogic a; // TODO new\nendmodule\n");
    let checked = ws.check(&[]).unwrap();
    sv_syntax::overlay::clear(&top);
    assert_eq!(rules(&checked), vec![("SV-STY-003", 3)], "line 4 is waived");
    assert_eq!(rules(&ws.check(&[]).unwrap()), vec![("SV-STY-003", 2)]);

    let _ = std::fs::remove_dir_all(&dir);
}
