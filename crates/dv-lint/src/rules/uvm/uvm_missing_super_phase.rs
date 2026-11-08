//! UVM-PHS-001 (UvmMissingSuperPhase): Overridden standard UVM phases (e.g.,
//! build_phase, connect_phase) in uvm_component must call
//! `super.<phase_name>(phase)` to ensure proper base-class configuration.

use crate::support::uvm_kinds::is_component;
use crate::{is_function_phase, is_task_phase, Severity, Sink};
use dv_model::Design;

pub(crate) fn uvm_missing_super_phase(d: &Design, csts: &[crate::syntax::Cst], out: &mut Sink) {
    let ix = crate::support::class_roles::Index::new(csts);
    for cls in d.classes() {
        if cls.is_parameterized || cls.name == "uvm_component" || !is_component(d, cls) {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || !(is_function_phase(&sub.name) || is_task_phase(&sub.name)) {
                continue;
            }
            if sub.calls.iter().any(|c| c.name == sub.name) {
                continue;
            }
            out.add(
                "UVM-PHS-001",
                "UvmMissingSuperPhase",
                Severity::Warning,
                format!("Method '{0}' in class '{1}' does not call super.{0}(phase).", sub.name, cls.name),
                sub.loc,
            )
            .fixes
            .extend(
                d.ancestors(cls.id)
                    .any(|a| a.methods.iter().any(|m| m.name == sub.name))
                    .then(|| ix.decl(sub.loc).and_then(|(c, n)| super_call_fix(c, n, &sub.name)))
                    .flatten(),
            );
        }
    }
}

/// Inserts `super.<phase>(<arg>);` as the first statement of the phase: after
/// the header and any leading declarations (SystemVerilog allows no
/// declaration after a statement), at the indent of the first item. Unsafe:
/// the base phase now runs, first. Only for a base class that declares the
/// method (so the call compiles) and a phase with one argument.
fn super_call_fix(c: &crate::syntax::Cst, decl: sv_syntax::NodeId, name: &str) -> Option<crate::fix::Fix> {
    use sv_syntax::SyntaxKind::*;
    if !matches!(c.kind(decl), FunctionDecl | TaskDecl) {
        return None;
    }
    let ports = c.tree.child(decl, PortList)?;
    let args: Vec<_> = c.tree.children(ports).filter(|&p| c.kind(p) == Port).collect();
    let [port] = args[..] else { return None };
    let arg = c.tree.child(port, Declarator).and_then(|d| c.tree.child(d, Name)).map(|n| c.tree.bare_text(n))?;
    let semi = c.toks(decl).find(|&t| c.text_of(t) == ";")?;
    let tok_end = |t: u32| {
        let k = &c.tree.tokens[t as usize];
        (!k.from_macro).then_some((k.origin.file, k.origin.offset as usize + k.text.len()))
    };
    let (file, mut at) = tok_end(semi)?;
    let src = c.tree.sources.get(file as usize)?.as_bytes();
    let indent_of = |t: u32| {
        let k = &c.tree.tokens[t as usize];
        let pos = k.origin.offset as usize;
        let mut start = pos;
        while start > 0 && src[start - 1] != b'\n' {
            start -= 1;
        }
        let mut end = start;
        while end < src.len() && matches!(src[end], b' ' | b'\t') {
            end += 1;
        }
        String::from_utf8_lossy(&src[start..end]).into_owned()
    };
    let mut indent = format!("{}    ", indent_of(c.first_tok(decl)?));
    let items: Vec<_> = c.tree.child(decl, Body).map(|b| c.tree.children(b).collect()).unwrap_or_default();
    for &item in &items {
        if c.kind(item) != DataDecl && c.kind(item) != TypedefDecl {
            break;
        }
        let (f, e) = tok_end(c.tree.token_bounds(item)?.1)?;
        if f != file {
            return None;
        }
        at = e;
    }
    if let Some(&first) = items.first() {
        indent = indent_of(c.first_tok(first)?);
    }
    let fix =
        c.file_fix(file, at, at, format!("\n{indent}super.{name}({arg});"), &format!("Call super.{name}() first"));
    crate::fix::with_safety(Some(fix), crate::fix::FixSafety::Unsafe)
}
