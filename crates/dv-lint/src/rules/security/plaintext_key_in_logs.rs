//! SEC-LOG-001 (PlaintextKeyInLogs): Detects plaintext logging of sensitive
//! cryptographic assets (keys, seeds, secrets) via $sformatf/$display.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use crate::support::ast::{Ast, call_site, source_text, string_value, subtree, unparen};

fn plaintext_key(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in subtree(c, c.tree.root) {
        let Some(call) = call_site(c, n) else { continue };
        let name = call.name.strip_prefix('$').unwrap_or(&call.name);
        if !matches!(name, "sformatf" | "display" | "write" | "writeh") || call.args.len() < 2 {
            continue;
        }
        let Some(Some(fmt)) = call.args.first().copied() else { continue };
        let Some(fmt) = string_value(c, unparen(c, fmt)) else { continue };
        if !fmt.contains('%') {
            continue;
        }
        for arg in call.args.iter().skip(1).copied().flatten() {
            let text = source_text(c, arg);
            let low = text.to_lowercase();
            if !["key", "secret", "seed", "token", "priv_"].iter().any(|w| low.contains(w)) {
                continue;
            }
            // The text keeps the parentheses; the location is inside them.
            let Some(at) = c.first_tok(unparen(c, arg)) else { continue };
            s.add(
                "SEC-LOG-001",
                "PlaintextKeyInLogs",
                Severity::Warning,
                format!(
                    "Potential plaintext logging of sensitive cryptographic asset '{text}' via ${name}(). Printing keys, seeds, or root-of-trust secrets in simulation \
                     transcripts violates hardware security standards (ISO/SAE 21434 / Common Criteria). \
                     Mask or omit sensitive secrets from regression logs."
                ),
                c.loc(at),
            );
            a.again(s, c, n);
        }
    }
}

pub(crate) fn ast_plaintext_key(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        plaintext_key(c, &Ast::new(d, c), s);
    }
}
