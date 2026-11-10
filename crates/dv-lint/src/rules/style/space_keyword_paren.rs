//! SV-STY-012 (SpaceKeywordParen): Requires a single space between control
//! keywords (if, for, foreach, while, case) and opening parenthesis.

use crate::support::source_layout::Buffers;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn space_keyword_paren(c: &Cst, s: &mut Sink) {
    let b = Buffers::new(c);
    let check = |s: &mut Sink, kw: u32, open: u32| {
        if b.of(kw) != b.of(open) {
            return;
        }
        let (k, o) = (&c.tree.tokens[kw as usize], &c.tree.tokens[open as usize]);
        let (ko, oo) =
            if k.from_macro { (k.written.offset, o.written.offset) } else { (k.origin.offset, o.origin.offset) };
        if ko + k.text.len() as u32 == oo {
            let kw_text = c.text_of(kw);
            s.add(
                "SV-STY-012",
                "SpaceKeywordParen",
                Severity::Info,
                format!(
                    "Missing space between '{kw_text}' keyword and opening parenthesis '('. Common SystemVerilog formatting requires a space (e.g. '{kw_text} (...)')."
                ),
                c.loc(kw),
            )
            .fixes
            .extend((!o.from_macro).then(|| c.file_fix(o.origin.file, oo as usize, oo as usize, " ", "Insert a space before '('")));
        }
    };
    let pairs = |n: NodeId, kws: &[&str]| -> Option<(u32, u32)> {
        let kw = c.toks(n).find(|&t| kws.contains(&c.text_of(t)))?;
        let open = c.toks(n).find(|&t| t > kw && c.text_of(t) == "(")?;
        Some((kw, open))
    };
    let mut found = Vec::new();
    for n in c.nodes(IfStmt) {
        found.extend(pairs(n, &["if"]));
    }
    for n in c.nodes(LoopStmt) {
        found.extend(pairs(n, &["while", "repeat", "for", "foreach"]));
    }
    for n in c.nodes(DoWhileStmt) {
        found.extend(pairs(n, &["while"]));
    }
    for n in c.nodes(CaseStmt) {
        found.extend(pairs(n, &["case", "casex", "casez"]));
    }
    for (kw, open) in found {
        check(s, kw, open);
    }
}
