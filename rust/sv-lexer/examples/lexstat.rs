//! Lexes every .sv/.svh/.v/.vh file under the given paths and checks that
//! the tokens give the file back byte for byte.
//!
//!     cargo run --release -p sv-lexer --example lexstat -- <dir-or-file>...
//!
//! Prints files, bytes, tokens, Unknown tokens (by file) and throughput;
//! exits 1 if any file does not round-trip.
use std::path::{Path, PathBuf};
use std::time::Instant;

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            collect(&e, out);
        }
    } else if matches!(p.extension().and_then(|e| e.to_str()), Some("sv" | "svh" | "v" | "vh")) {
        out.push(p.to_path_buf());
    }
}

fn main() {
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        collect(Path::new(&a), &mut files);
    }
    let (mut bytes, mut tokens, mut unknown, mut not_utf8, mut broken) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut lex_time = 0f64;
    let mut worst: Vec<(usize, String, String)> = Vec::new();
    for f in &files {
        let raw = std::fs::read(f).unwrap_or_default();
        let src = match String::from_utf8(raw) {
            Ok(s) => s,
            Err(e) => {
                not_utf8 += 1;
                String::from_utf8_lossy(e.as_bytes()).into_owned()
            }
        };
        let t = Instant::now();
        let toks = sv_lexer::lex(&src);
        lex_time += t.elapsed().as_secs_f64();
        bytes += src.len();
        tokens += toks.len();
        if toks.iter().map(|t| t.text(&src)).collect::<String>() != src {
            broken += 1;
            eprintln!("NOT LOSSLESS: {}", f.display());
        }
        let unk: Vec<_> = toks.iter().filter(|t| t.kind == sv_lexer::Kind::Unknown).collect();
        if !unk.is_empty() {
            unknown += unk.len();
            let first = unk[0].text(&src).chars().take(30).collect::<String>();
            worst.push((unk.len(), f.display().to_string(), first));
        }
    }
    worst.sort_by(|a, b| b.0.cmp(&a.0));
    for (n, f, first) in worst.iter().take(15) {
        println!("  {n:5} unknown  {f}  first: {first:?}");
    }
    println!(
        "files={} bytes={} tokens={} unknown={} files_with_unknown={} not_utf8={} not_lossless={} lex={:.1}ms ({:.0} MB/s)",
        files.len(),
        bytes,
        tokens,
        unknown,
        worst.len(),
        not_utf8,
        broken,
        lex_time * 1e3,
        bytes as f64 / 1e6 / lex_time.max(1e-9)
    );
    std::process::exit(if broken == 0 { 0 } else { 1 });
}
