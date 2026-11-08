//! dv-lsp: a SystemVerilog/UVM language server for code browsing on the
//! native front end (sv-syntax). It answers document and workspace symbols,
//! go to definition/declaration/type definition, references, highlights,
//! hover and completion. Lint diagnostics and quick fixes stay with
//! `uvmcheck --lsp`; editors run both side by side.
//!
//!     dv-lsp            # LSP over stdio
//!     dv-lsp --index DIR...   # index and print a summary (for testing)

mod index;
mod json;
mod server;
mod text;
mod workspace;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("dv-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.first().map(String::as_str) == Some("--index") {
        let roots: Vec<std::path::PathBuf> = args[1..].iter().map(std::path::PathBuf::from).collect();
        let mut ws = workspace::Workspace::default();
        let t0 = std::time::Instant::now();
        let n = ws.scan(&roots, &[]);
        let syms: usize = ws.files.iter().flatten().map(|f| f.symbols.len()).sum();
        println!("{} files, {} symbols, {} names, {} ms", n, syms, ws.globals.len(), t0.elapsed().as_millis());
        return;
    }
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut s = server::Server::new(Box::new(std::io::stdout()));
    std::process::exit(s.run(&mut input));
}
