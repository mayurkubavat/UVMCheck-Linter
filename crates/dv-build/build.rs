// dv-build has no slang feature of its own, but `--workspace --features slang`
// turns on dv-model's through feature unification, and then the binary and
// its tests load libuvmcheck_model. Give them an rpath to it (same lookup as
// sv-slang: $UVMCHECK_BUILD_DIR, else <repo>/build-model) whenever the
// library is there; an rpath the binary never uses is harmless.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=UVMCHECK_BUILD_DIR");
    let dir = match std::env::var_os("UVMCHECK_BUILD_DIR") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build-model"),
    };
    let dir = dir.canonicalize().unwrap_or(dir);
    let found =
        ["libuvmcheck_model.so", "libuvmcheck_model.dylib", "uvmcheck_model.dll"].iter().any(|f| dir.join(f).exists());
    if found {
        // The binary and its unit-test harness (this crate has no other targets).
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }
}
