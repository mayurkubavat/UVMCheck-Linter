//! Lossless SystemVerilog syntax tree (R2 of the Rust front end).
//!
//! ```
//! let tree = sv_syntax::parse_source("module m; endmodule\n", "m.sv", Default::default());
//! assert!(tree.errors.is_empty());
//! assert_eq!(tree.file_text(0), "module m; endmodule\n");
//! ```

mod assertions;
mod blocks;
mod checks;
mod expr;
mod exprs;
mod grammar;
mod kinds;
mod misc;
mod outline;
mod parser;
mod tree;

use std::path::Path;

pub use exprs::canonical_exprs;
pub use kinds::SyntaxKind;
pub use outline::{outline, OutlineEntry};
pub use sv_lexer::Kind as TokenKind;
pub use sv_preproc::{Options, Origin};
/// A built-in data type keyword (`int`, `bit`, `string`, ...).
pub fn is_builtin_type(s: &str) -> bool {
    grammar::BUILTIN_TYPES.contains(&s)
}

pub use tree::{Child, Node, NodeId, SyntaxError, Token, Tree};

/// Preprocesses and parses one file.
pub fn parse_file(path: &Path, opts: Options) -> Tree {
    let out = sv_preproc::Preprocessor::new(opts).run_file(path);
    parse_output(out)
}

/// Preprocesses and parses `src` as if read from a file called `name`.
pub fn parse_source(src: &str, name: &str, opts: Options) -> Tree {
    let out = sv_preproc::Preprocessor::new(opts).run_source(src, name);
    parse_output(out)
}

/// Parses tokens the preprocessor produced.
pub fn parse_output(out: sv_preproc::Output) -> Tree {
    let tokens = tree::tokens_from(&out);
    let mut p = parser::Parser::new(&tokens);
    grammar::source_file(&mut p);
    let errors = std::mem::take(&mut p.errors);
    let events = std::mem::take(&mut p.events);
    drop(p);
    let (nodes, root) = parser::build(events);
    let mut tree =
        Tree { tokens, nodes, root, files: out.files, sources: out.sources, errors, pp_diagnostics: out.diagnostics };
    let mut more = checks::run(&tree);
    tree.errors.append(&mut more);
    tree.errors.sort_by_key(|e| e.token);
    tree
}
