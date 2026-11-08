// With the slang feature the model_dump example also loads the Slang-backed
// model to compare against; let them find libuvmcheck_model at run time (same lookup as
// sv-slang: $UVMCHECK_BUILD_DIR, else <repo>/build-model).
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
    // Every target that links: examples, tests and the lib's unit tests.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
}
