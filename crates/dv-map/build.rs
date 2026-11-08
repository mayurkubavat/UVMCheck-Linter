// Lets this crate's tests find libuvmcheck_model at run time.
fn main() {
    if let Ok(dir) = std::env::var("DEP_UVMCHECK_MODEL_LIBDIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
