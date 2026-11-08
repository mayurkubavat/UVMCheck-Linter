// With the slang feature the binary can load through libuvmcheck_model; let
// it find the library at run time (same lookup as sv-slang). Without it
// nothing here refers to the C++ build.
//
// Also writes $OUT_DIR/explain_data.rs for `--explain`: each rule's tested
// example (testdata/rules/<category>/<ID>/bad.sv and good.sv) and the options
// the native rules read (`options::get("ID", "key", default)` in src/), as
// tools/codegen/gen_rule_extras.py does for the C++ binary.
use std::path::{Path, PathBuf};

fn main() {
    explain_data();
    println!("cargo:rerun-if-env-changed=UVMCHECK_BUILD_DIR");
    if std::env::var_os("CARGO_FEATURE_SLANG").is_none() {
        return;
    }
    let dir = match std::env::var_os("UVMCHECK_BUILD_DIR") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build-model"),
    };
    let dir = dir.canonicalize().unwrap_or(dir);
    // Every target that links: the binaries, the tests and the lib's unit tests.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
}

fn explain_data() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let testdata = manifest.join("../../testdata/rules");
    let src = manifest.join("src");
    println!("cargo:rerun-if-changed={}", testdata.display());
    println!("cargo:rerun-if-changed={}", src.display());
    let lit = |s: &str| format!("{s:?}");
    let mut out = String::from(
        "/// (rule, test folder, bad.sv, good.sv)\npub(crate) const EXAMPLES: &[(&str, &str, &str, &str)] = &[\n",
    );
    let mut cases: Vec<(String, PathBuf)> = Vec::new();
    for cat in sorted_dir(&testdata) {
        for case in sorted_dir(&cat) {
            if case.is_dir() {
                let rel = format!(
                    "testdata/rules/{}/{}",
                    cat.file_name().unwrap_or_default().to_string_lossy(),
                    case.file_name().unwrap_or_default().to_string_lossy()
                );
                cases.push((rel, case));
            }
        }
    }
    for (rel, case) in &cases {
        let read = |n: &str| String::from_utf8_lossy(&std::fs::read(case.join(n)).unwrap_or_default()).into_owned();
        let id = case.file_name().unwrap_or_default().to_string_lossy().into_owned();
        out.push_str(&format!(
            "    ({}, {}, {}, {}),\n",
            lit(&id),
            lit(rel),
            lit(&read("bad.sv")),
            lit(&read("good.sv"))
        ));
    }
    out.push_str("];\n\n/// (rule, option, default) as the native rules read them.\npub(crate) const OPTIONS: &[(&str, &str, &str)] = &[\n");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files.sort();
    let mut seen: Vec<(String, String, String)> = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("options::get(") {
            rest = &rest[at + "options::get(".len()..];
            let Some(close) = rest.find(')') else { break };
            let parts: Vec<&str> = rest[..close].split(',').map(str::trim).collect();
            if parts.len() == 3 && parts[0].starts_with('"') && parts[1].starts_with('"') {
                let entry = (
                    parts[0].trim_matches('"').to_string(),
                    parts[1].trim_matches('"').to_string(),
                    parts[2].to_string(),
                );
                if !seen.contains(&entry) {
                    seen.push(entry);
                }
            }
        }
    }
    for (r, k, d) in &seen {
        out.push_str(&format!("    ({}, {}, {}),\n", lit(r), lit(k), lit(d)));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("explain_data.rs");
    if std::fs::read_to_string(&dest).ok().as_deref() != Some(out.as_str()) {
        let _ = std::fs::write(dest, out);
    }
}

fn sorted_dir(d: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(d).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    v.sort();
    v
}

fn rust_files(d: &Path, out: &mut Vec<PathBuf>) {
    for p in sorted_dir(d) {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}
