//! DPI-TYP-001 (DpiTypeWidthAlignment): Disallow 4-state types (integer, reg)
//! in DPI-C declarations. 4-state types require complex svLogicVecVal structs
//! in C; use 2-state portable types (int, longint, byte) for C99 ABI
//! compatibility.

use crate::support::ast::last_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

/// slang's `toString()` of the tokens `a..=b`: each with its leading trivia.
fn span_text(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

pub(crate) fn dpi_type_width(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DpiImport) {
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
        let Some(name) = c.tree.child(n, Name) else { continue };
        let Some(end) = last_tok(c, n) else { continue };
        let end = if c.text_of(end) == ";" { end - 1 } else { end };
        let proto = span_text(c, kw, end);
        if !(proto.contains("integer") || proto.contains(" reg ") || proto.contains("(reg ")) {
            continue;
        }
        let Some(at) = c.first_tok(name) else { continue };
        s.add(
            "DPI-TYP-001",
            "DpiTypeWidthAlignment",
            Severity::Warning,
            format!(
                "DPI-C function '{}' uses 4-state type ('integer'/'reg') across the C boundary. \
                 In IEEE 1800 § 35.5, 4-state types require complex 'svLogicVecVal' struct layouts in C/C++ FFI. \
                 Use portable 2-state types ('int' for int32_t, 'longint' for int64_t, 'byte' for int8_t) for direct C99 ABI compatibility.",
                c.text_of(at)
            ),
            c.loc(at),
        );
    }
}
