//! dv-lint over a compile of several files, each its own syntax tree (the
//! default without `--single-unit`): rules must read a node with the tree
//! it belongs to.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A class declared in one file and constructed in another is still found.
#[test]
fn rules_read_declarations_of_other_files() {
    let dir = root().join("testdata/multifile");
    let args = [dir.join("a.sv").display().to_string(), dir.join("b.sv").display().to_string()];
    let (d, trees) = sv_sema::load_with_trees(&args).unwrap();
    assert_eq!(trees.len(), 2);
    let (files, found) = dv_lint::lint_all(&d, &trees, |_| true);
    let oop13: Vec<_> = found
        .iter()
        .filter(|f| f.rule == "SV-OOP-013")
        .map(|f| {
            let (file, line, _) = f.position_in(&files);
            (file, line)
        })
        .collect();
    assert!(
        oop13.iter().any(|(f, l)| f.ends_with("b.sv") && *l == 10),
        "SV-OOP-013 not reported at b.sv:10: {oop13:?}"
    );
}

/// A file that cannot be read is reported, not a crash.
#[test]
fn unreadable_file_is_reported() {
    let dir = root().join("testdata/multifile");
    let trees = sv_syntax::parse_files(&[dir.join("a.sv"), dir.join("missing.sv")], Default::default());
    let d = &trees[1].pp_diagnostics;
    assert!(d.iter().any(|x| x.message.starts_with("cannot read")), "{d:?}");
    assert!(d.iter().all(|x| (x.origin.file as usize) < trees[1].files.len()));
}

/// Every rule runs on rule fixtures compiled together, one tree per file,
/// without a panic. Every eighth fixture keeps the debug-build test short
/// while still mixing every family.
#[test]
fn every_rule_runs_on_fixtures_as_one_compile() {
    let r = root();
    let mut args = vec![format!("+incdir+{}", r.join("testdata/common").display())];
    let mut files: Vec<PathBuf> = std::fs::read_dir(r.join("testdata/rules"))
        .unwrap()
        .flatten()
        .flat_map(|area| std::fs::read_dir(area.path()).into_iter().flatten().flatten())
        .flat_map(|rule| ["bad.sv", "good.sv"].map(|f| rule.path().join(f)))
        .filter(|p| p.exists())
        .collect();
    files.sort();
    assert!(files.len() > 500, "only {} fixtures", files.len());
    let files: Vec<PathBuf> = files.into_iter().step_by(8).collect();
    args.extend(files.iter().map(|p| p.display().to_string()));
    let (d, trees) = sv_sema::load_with_trees(&args).unwrap();
    assert_eq!(trees.len(), files.len());
    let (_, found) = dv_lint::lint_all(&d, &trees, |_| true);
    assert!(!found.is_empty());
    // The severity a rule declares is the worst it reports: presets, the
    // ERROR budget and the catalog go by it. (A rule may report a milder
    // tier, as SV-STY-010 does for decimal literals.)
    let declared: std::collections::HashMap<&str, dv_lint::Severity> =
        dv_lint::catalog::entries().into_iter().map(|e| (e.id, e.severity)).collect();
    for f in &found {
        let Some(&worst) = declared.get(f.rule) else { panic!("{} is reported but not in the catalog", f.rule) };
        assert!(f.severity <= worst, "{} reports {} but declares {}", f.rule, f.severity.as_str(), worst.as_str());
    }
}
