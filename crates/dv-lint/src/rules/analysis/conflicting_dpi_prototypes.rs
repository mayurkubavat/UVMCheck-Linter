//! DPI-PRT-001 (ConflictingDpiPrototypes): Flags a C function imported more
//! than once with different prototypes (`import "DPI-C" function int f(int)`
//! here, `... function void f(longint)` there). The C side has one
//! signature, so at least one import calls it with the wrong arguments or
//! return type: the stack is corrupted at run time, not at compile time.
//! Imports are matched by their C name (`import "DPI-C" c_name = sv_name`).

use std::collections::BTreeMap;

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The prototype without names: `function int (input int, output bit [7:0])`.
fn signature(c: &Cst, n: NodeId) -> String {
    let kind = c.toks(n).map(|t| c.text_of(t)).find(|t| matches!(*t, "function" | "task")).unwrap_or("function");
    let ret = c.tree.child(n, ReturnType).map(|r| c.trimmed(r)).unwrap_or_default();
    let mut ports = Vec::new();
    let mut dir = "input".to_string();
    if let Some(pl) = c.tree.child(n, PortList) {
        for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
            if let Some(d) =
                c.toks(p).map(|t| c.text_of(t)).find(|t| matches!(*t, "input" | "output" | "inout" | "ref"))
            {
                dir = d.to_string();
            }
            let ty = c.tree.child(p, DataType).map(|t| c.trimmed(t)).unwrap_or_else(|| "logic".into());
            let dims: String = c
                .tree
                .child(p, Declarator)
                .map(|d| c.tree.children(d).filter(|&k| c.kind(k) == Dim).map(|k| c.trimmed(k)).collect())
                .unwrap_or_default();
            ports.push(format!("{dir} {ty}{}", if dims.is_empty() { String::new() } else { format!(" {dims}") }));
        }
    }
    let norm = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
    norm(format!("{kind} {ret} ({})", ports.join(", ")))
}

/// The C name of an import: `c_name = sv_name`, else its SV name.
fn c_name(c: &Cst, n: NodeId) -> Option<String> {
    let toks: Vec<&str> = c.toks(n).map(|t| c.text_of(t)).collect();
    if let Some(i) = toks.iter().position(|t| *t == "=") {
        if i > 0 && !toks[i - 1].starts_with('"') {
            return Some(toks[i - 1].to_string());
        }
    }
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x))
}

pub(crate) fn conflicting_dpi_prototypes(_: &Design, csts: &[Cst], out: &mut Sink) {
    // C name -> (signature -> first import with it).
    let mut by_name: BTreeMap<String, Vec<(String, Loc, String)>> = BTreeMap::new();
    for c in csts {
        for n in c.nodes(DpiImport) {
            let Some(name) = c_name(c, n) else { continue };
            let Some(at) = c.tree.child(n, Name).and_then(|x| c.first_tok(x)) else { continue };
            let sig = signature(c, n);
            let loc = c.loc(at);
            let file = c
                .tree
                .files
                .get(c.tree.tokens[at as usize].origin.file as usize)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let v = by_name.entry(name).or_default();
            if !v.iter().any(|(s, _, _)| *s == sig) {
                v.push((sig, loc, format!("{}:{}", file, loc.line)));
            }
        }
    }
    for (name, sigs) in by_name {
        let Some((first, _, at)) = sigs.first() else { continue };
        for (sig, loc, _) in sigs.iter().skip(1) {
            out.add(
                "DPI-PRT-001",
                "ConflictingDpiPrototypes",
                Severity::Error,
                format!(
                    "DPI-C function '{name}' is imported here as '{sig}' but as '{first}' at {at}. The C function has one signature, so \
one of the imports calls it with the wrong argument or return types, which corrupts memory at run time. Import it once, in a \
package, and import that package where it is used."
                ),
                *loc,
            );
        }
    }
}
