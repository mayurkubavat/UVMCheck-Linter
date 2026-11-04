//! Prints a file's outline in the form of the Slang reference tool
//! (build/uvmcheck_cst_outline), then `errors N`. With --check, also
//! verifies that every file round-trips through the tree byte for byte.
//! With --exprs, prints the canonical operator expressions instead
//! (the form of build/uvmcheck_cst_exprs).
//!
//!     cargo run --release -p sv-syntax --example outline -- [+incdir+DIR]... [+define+X[=V]]... [--check] [--errors] FILE
use std::path::PathBuf;

fn main() {
    let mut opts = sv_syntax::Options::default();
    let mut file = None;
    let (mut check, mut show_errors, mut exprs) = (false, false, false);
    for a in std::env::args().skip(1) {
        if let Some(d) = a.strip_prefix("+incdir+") {
            opts.include_dirs.push(PathBuf::from(d));
        } else if let Some(d) = a.strip_prefix("+define+") {
            opts.defines.push(d.to_string());
        } else if a == "--check" {
            check = true;
        } else if a == "--exprs" {
            exprs = true;
        } else if a == "--errors" {
            show_errors = true;
        } else {
            file = Some(PathBuf::from(a));
        }
    }
    let file = file.expect("a file to parse");
    let t = std::time::Instant::now();
    let tree = sv_syntax::parse_file(&file, opts);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let mut s = String::new();
    if exprs {
        for e in sv_syntax::canonical_exprs(&tree) {
            s.push_str(&e);
            s.push('\n');
        }
    } else {
        for e in sv_syntax::outline(&tree) {
            s.push_str(e.kind);
            s.push(' ');
            s.push_str(&e.name);
            s.push('\n');
        }
    }
    print!("{s}");
    println!("errors {}", tree.errors.len());
    if show_errors {
        for e in &tree.errors {
            let (f, off) = tree.tokens.get(e.token as usize).map_or((0, 0), |t| (t.origin.file, t.origin.offset));
            let src = &tree.sources[f as usize];
            let line = 1 + src[..off as usize].matches('\n').count();
            eprintln!("{}:{}: {}", tree.files[f as usize].display(), line, e.message);
        }
    }
    if check {
        let mut bad = 0;
        for f in 0..tree.files.len() as u32 {
            if tree.file_text(f) != *tree.sources[f as usize] {
                bad += 1;
                eprintln!("round-trip differs: {}", tree.files[f as usize].display());
            }
        }
        eprintln!("{} files, {} tokens, {} nodes, {:.1} ms, round-trip failures {}", tree.files.len(), tree.tokens.len(), tree.nodes.len(), ms, bad);
    }
}
