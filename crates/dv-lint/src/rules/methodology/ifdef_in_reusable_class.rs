//! SV-CNS-003 (IfdefInReusableClass): Flags `ifdef, `ifndef and `elsif inside
//! class bodies, apart from simulator and UVM version shims.

use std::collections::HashSet;
use dv_model::Design;
use sv_lexer::Kind;
use sv_syntax::Tree;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::defines_packages::{Files, Lexed, origin_loc, user_tok};

/// A conditional directive (token `i` of file `f`) was seen by the
/// preprocessor. A skipped range runs from the first branch not taken, so a
/// later `elsif, `else or `endif of the same conditional inside it is
/// live; one of a conditional nested in skipped text is not.
fn live_conditional(tree: &Tree, lx: &Lexed, f: u32, i: usize) -> bool {
    let off = lx.toks[i].1;
    let closes = matches!(lx.text(i), "`elsif" | "`else" | "`endif");
    tree.skipped.iter().filter(|&&(sf, s, e)| sf == f && s < off && off < e).all(|&(_, s, _)| {
        let Ok(j) = lx.toks.binary_search_by_key(&s, |t| t.1) else { return false };
        let mut depth = 1;
        for k in j + 1..i {
            if lx.toks[k].0 != Kind::Directive {
                continue;
            }
            match lx.text(k) {
                "`ifdef" | "`ifndef" => depth += 1,
                "`endif" => depth -= 1,
                _ => {}
            }
        }
        closes && depth == 1
    })
}

const SHIM_PREFIXES: &[&str] = &[
    "UVM_", "VCS", "QUESTA", "MODEL_TECH", "MTI", "XCELIUM", "INCA", "NCSIM", "CADENCE", "RIVIERA", "ALDEC",
    "VERILATOR", "SYNTHESIS", "SYNOPSYS", "IUS", "DSIM", "IVERILOG",
];

pub(crate) fn ifdef_in_reusable_class(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c);
        let mut seen = HashSet::new();
        for cls in c.nodes(ClassDecl) {
            let Some((first, last)) = c.tree.token_bounds(cls) else { continue };
            if !user_tok(c, &fs, first) {
                continue;
            }
            let name = c.tree.child(cls, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
            let ft = &c.tree.tokens[first as usize];
            let f = ft.origin.file;
            let from = ft.origin.offset + ft.text.len() as u32;
            // Directives in the trivia of the class's tokens after the first,
            // up to the last token.
            let lt = &c.tree.tokens[last as usize];
            let to = if lt.origin.file == f && !lt.from_macro {
                lt.origin.offset
            } else {
                c.tree.sources[f as usize].len() as u32
            };
            let lx = &fs.lexed[f as usize];
            for i in 0..lx.toks.len() {
                let (k, at, ..) = lx.toks[i];
                if at < from || at >= to {
                    continue;
                }
                if k != Kind::Directive || !matches!(lx.text(i), "`ifdef" | "`ifndef" | "`elsif") || !live_conditional(c.tree, lx, f, i) {
                    continue;
                }
                if !seen.insert((f, at)) {
                    continue;
                }
                let names: Vec<&str> = lx
                    .rest_of_line(i)
                    .filter(|&j| matches!(lx.toks[j].0, Kind::Identifier | Kind::EscapedIdentifier))
                    .map(|j| lx.text(j))
                    .collect();
                if names.is_empty() || names.iter().any(|n| SHIM_PREFIXES.iter().any(|p| n.starts_with(p))) {
                    continue;
                }
                let what = if names.len() == 1 { format!("`{}", names[0]) } else { "macros".to_string() };
                out.add(
                    "SV-CNS-003",
                    "IfdefInReusableClass",
                    Severity::Warning,
                    format!(
                        "Class '{name}' changes its members or code with {what}. Each define gives a different class that has to be \
compiled and tested on its own, and a reusable component cannot be configured per instance this way. Use a config object \
field, a parameter or a factory override."
                    ),
                    origin_loc(c, f, at),
                );
            }
        }
    }
}
