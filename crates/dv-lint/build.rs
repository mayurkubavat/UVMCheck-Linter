// With the slang feature the binary can load through libuvmcheck_model; let
// it find the library at run time (same lookup as sv-slang). Without it
// nothing here refers to the C++ build.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=UVMCHECK_BUILD_DIR");
    if std::env::var_os("CARGO_FEATURE_SLANG").is_none() {
        return;
    }
    let dir = match std::env::var_os("UVMCHECK_BUILD_DIR") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build-model"),
    };
    let dir = dir.canonicalize().unwrap_or(dir);
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{}", dir.display());
}
