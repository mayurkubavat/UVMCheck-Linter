//! Prints the preprocessed tokens of one file, one per line, in the same
//! form as the Slang reference tool (build/uvmcheck_pp_dump), so the two
//! can be diffed:
//!
//!     cargo run --release -p sv-preprocessor --example ppdump -- [+incdir+DIR]... [+define+X[=V]]... FILE
use std::path::PathBuf;

fn main() {
    let mut opts = sv_preprocessor::Options::default();
    let mut file = None;
    let mut diags = false;
    for a in std::env::args().skip(1) {
        if let Some(d) = a.strip_prefix("+incdir+") {
            opts.include_dirs.push(PathBuf::from(d));
        } else if let Some(d) = a.strip_prefix("+define+") {
            opts.defines.push(d.to_string());
        } else if a == "--diags" {
            diags = true;
        } else {
            file = Some(PathBuf::from(a));
        }
    }
    let file = file.expect("a file to preprocess");
    let t = std::time::Instant::now();
    let out = sv_preprocessor::Preprocessor::new(opts).run_file(&file);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    if diags {
        for d in &out.diagnostics {
            let f = out.files.get(d.origin.file as usize).map(|p| p.display().to_string()).unwrap_or_default();
            eprintln!("{f}@{}: {}", d.origin.offset, d.message);
        }
        eprintln!("{} tokens, {} files, {:.1} ms", out.tokens.len(), out.files.len(), ms);
    }
    let mut s = String::with_capacity(out.tokens.len() * 8);
    for t in &out.tokens {
        s.push_str(&t.text.replace('\n', "\\n").replace('\r', "\\r"));
        s.push('\n');
    }
    print!("{s}");
}
