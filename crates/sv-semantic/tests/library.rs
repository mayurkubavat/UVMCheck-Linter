//! Library search: -v files, -y directories and +libext+.

use std::fs;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sv-semantic-lib-{}-{name}", std::process::id()));
    fs::remove_dir_all(&d).ok();
    fs::create_dir_all(d.join("cells")).unwrap();
    fs::write(d.join("top.sv"), "module top; cellA u1(); cellB #(.W(2)) u2(); endmodule\n").unwrap();
    fs::write(d.join("lib.v"), "module cellA; endmodule\nmodule spare; endmodule\n").unwrap();
    fs::write(d.join("cells/cellB.sv"), "module cellB #(parameter W = 1); cellC c(); endmodule\n").unwrap();
    fs::write(d.join("cells/cellC.sv"), "module cellC; endmodule\n").unwrap();
    // Without +libext+ only the bare name is tried.
    fs::write(d.join("cells/cellB"), "module cellB #(parameter W = 1); endmodule\n").unwrap();
    d
}

fn hier(args: &[String]) -> Vec<String> {
    let (trees, _) = sv_semantic::parse_args(args).unwrap();
    sv_semantic::elaborate_full(&trees, &[]).hierarchy
}

#[test]
fn v_files_and_y_dirs_resolve_instances() {
    let d = scratch("resolve");
    let p = |s: &str| d.join(s).display().to_string();
    let args = vec!["-v".into(), p("lib.v"), "-y".into(), p("cells"), "+libext+.v+.sv".into(), p("top.sv")];
    let h = hier(&args);
    assert!(h.contains(&"top instance u1 : cellA".to_string()), "{h:?}");
    assert!(h.contains(&"top instance u2 : cellB".to_string()), "{h:?}");
    // cellB's own instance is found too, and library modules are never tops.
    assert!(h.contains(&"top.u2 instance c : cellC".to_string()), "{h:?}");
    assert!(!h.iter().any(|l| l.contains("spare")), "{h:?}");
    let (trees, _) = sv_semantic::parse_args(&args).unwrap();
    assert_eq!(trees.iter().filter(|t| t.library).count(), 3);
    fs::remove_dir_all(&d).ok();
}

#[test]
fn without_libext_only_the_bare_name_is_tried() {
    let d = scratch("bare");
    let p = |s: &str| d.join(s).display().to_string();
    let h = hier(&["-y".into(), p("cells"), p("top.sv")]);
    assert!(h.contains(&"top instance u2 : cellB".to_string()), "{h:?}");
    // The bare cellB instantiates nothing, and cellA is not in the directory.
    assert!(!h.iter().any(|l| l.contains("cellC")), "{h:?}");
    fs::remove_dir_all(&d).ok();
}

#[test]
fn a_source_definition_wins_over_the_library() {
    let d = scratch("wins");
    let p = |s: &str| d.join(s).display().to_string();
    fs::write(d.join("own.sv"), "module cellA; endmodule\n").unwrap();
    let args = vec!["-v".into(), p("lib.v"), p("own.sv"), p("top.sv")];
    let (trees, _) = sv_semantic::parse_args(&args).unwrap();
    assert_eq!(trees.iter().filter(|t| t.library).count(), 0);
    fs::remove_dir_all(&d).ok();
}
