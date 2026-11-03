// Finds libuvmcheck_model from the CMake build: $UVMCHECK_BUILD_DIR, else
// <repo>/build-model (cmake --preset model). Exposes its directory to dependents as DEP_UVMCHECK_MODEL_LIBDIR
// so they can set an rpath (see dv-model/build.rs).
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=UVMCHECK_BUILD_DIR");
    let dir = match std::env::var_os("UVMCHECK_BUILD_DIR") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build-model"),
    };
    let dir = dir.canonicalize().unwrap_or(dir);
    let found = ["libuvmcheck_model.so", "libuvmcheck_model.dylib", "uvmcheck_model.dll"]
        .iter()
        .any(|f| dir.join(f).exists());
    if !found {
        panic!(
            "libuvmcheck_model not found in {}; build it first (cmake --preset model && cmake --build build-model --target uvmcheck_model) \
             or set UVMCHECK_BUILD_DIR",
            dir.display()
        );
    }
    println!("cargo:rerun-if-changed={}", dir.join("libuvmcheck_model.so").display());
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=uvmcheck_model");
    println!("cargo:libdir={}", dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
}
