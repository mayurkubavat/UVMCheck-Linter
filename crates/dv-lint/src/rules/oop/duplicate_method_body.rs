//! SV-DUP-001 (DuplicateMethodBody): Detects identical or copy-pasted
//! subroutine method bodies across different classes.

use crate::support::class_items::decl_name;
use crate::support::data_items::{methods, routine_name};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// A node's text exactly as its file holds it, from its first token to the
/// end of its last; `None` when either end comes from a macro.
fn file_span(c: &Cst, n: NodeId) -> Option<(u32, String)> {
    let (_, b) = c.tree.token_bounds(n)?;
    // slang's FunctionDeclarationSyntax starts at `function` / `task`,
    // after the class method's qualifiers.
    let a = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task"))?;
    let (ta, tb) = (&c.tree.tokens[a as usize], &c.tree.tokens[b as usize]);
    if ta.from_macro || tb.from_macro || ta.origin.file != tb.origin.file {
        return None;
    }
    let src = &c.tree.sources[ta.origin.file as usize];
    let (s0, e0) = (ta.origin.offset as usize, tb.origin.offset as usize + tb.text.len());
    (e0 > s0 && e0 <= src.len()).then(|| (ta.origin.file, src[s0..e0].to_string()))
}

fn normalize_body(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut line = false;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if line {
            if ch == b'\n' {
                line = false;
            }
        } else if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            line = true;
        } else if !matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
            out.push(ch);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn duplicate_body(_: &Design, csts: &[Cst], s: &mut Sink) {
    let mut known: HashMap<String, (String, String, Loc)> = HashMap::new();
    for c in csts {
        for cls in crate::support::class_items::class_decls(c) {
            if crate::support::class_items::is_parameterized(c, cls) {
                continue;
            }
            let Some((cname, _)) = decl_name(c, cls) else { continue };
            for f in methods(c, cls) {
                let name = routine_name(c, f);
                if name == "new" || name.contains("::") {
                    continue;
                }
                let Some(name_tok) = c.tree.child(f, Name).and_then(|n| c.tree.token_bounds(n)).map(|x| x.1) else {
                    continue;
                };
                let Some((_, raw)) = file_span(c, f) else { continue };
                let norm = normalize_body(&raw);
                if norm.len() < 50 {
                    continue;
                }
                match known.get(&norm) {
                    Some((kc, km, kl)) => {
                        if *kc != cname {
                            let f = s.add(
                                "SV-DUP-001",
                                "DuplicateMethodBody",
                                Severity::Warning,
                                format!(
                                    "Subroutine '{name}' in class '{cname}' has an identical body to subroutine '{km}' in class '{kc}'. Refactor duplicate code into a common base class or shared package utility."
                                ),
                                c.loc(name_tok),
                            );
                            f.related.push((*kl, format!("first copy, {kc}::{km}")));
                        }
                    }
                    None => {
                        known.insert(norm, (cname.clone(), name.clone(), c.loc(name_tok)));
                    }
                }
            }
        }
    }
}
