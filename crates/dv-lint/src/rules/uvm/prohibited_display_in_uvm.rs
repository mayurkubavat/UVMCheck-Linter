//! UVM-LOG-002 (ProhibitedDisplayInUvm): Disallow direct
//! $display/$write/$strobe calls inside UVM classes (components, objects,
//! sequences).

use crate::support::uvm_classes::{call_name, calls_in, derived, is_component, is_sequence, Ix};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

const DISPLAYS: [&str; 12] = [
    "$display",
    "$displayh",
    "$displayb",
    "$displayo",
    "$write",
    "$writeh",
    "$writeb",
    "$writeo",
    "$strobe",
    "$strobeh",
    "$strobeb",
    "$strobeo",
];

pub(crate) fn display_in_uvm(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        let id = cls.id;
        let uvm = derived(d, id, "uvm_report_object")
            || is_component(d, id)
            || is_sequence(d, id)
            || derived(d, id, "uvm_object");
        if !uvm {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            for n in calls_in(c, at.node) {
                let name = call_name(c, n);
                if !DISPLAYS.contains(&name) {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-LOG-002",
                    "ProhibitedDisplayInUvm",
                    Severity::Warning,
                    format!(
                        "Direct system task '{name}' called in UVM class '{}'. Direct display tasks bypass UVM report catchers, severity filtering (+uvm_set_verbosity), and regression log routing. Use `uvm_info or `uvm_error instead.",
                        cls.name
                    ),
                    c.loc(t),
                )
                .fixes
                .extend(info_fix(c, n, name));
            }
        }
    }
}

/// `$display("..", args);` as a statement becomes
/// `` `uvm_info(get_type_name(), $sformatf("..", args), UVM_LOW) ``. Unsafe:
/// the message now obeys verbosity (UVM_LOW) and report catchers.
fn info_fix(c: &Cst, call: sv_syntax::NodeId, name: &str) -> Option<crate::fix::Fix> {
    use sv_syntax::SyntaxKind::*;
    if name != "$display" {
        return None;
    }
    let stmt = c.parent(call).filter(|&p| c.kind(p) == ExprStmt)?;
    let first_arg = crate::support::uvm_classes::args(c, call).first().copied().flatten()?;
    if c.kind(c.strip(first_arg)) != Literal || !c.tree.bare_text(c.strip(first_arg)).starts_with('"') {
        return None;
    }
    let (a, b) = c.tree.token_bounds(stmt)?;
    let (ca, cb) = c.tree.token_bounds(call)?;
    let toks = &c.tree.tokens;
    if [a, b, ca, cb].iter().any(|&t| toks[t as usize].from_macro)
        || toks[ca as usize].origin.file != toks[cb as usize].origin.file
    {
        return None;
    }
    let src = c.tree.sources.get(toks[ca as usize].origin.file as usize)?;
    let text = src.get(
        toks[ca as usize].origin.offset as usize
            ..toks[cb as usize].origin.offset as usize + toks[cb as usize].text.len(),
    )?;
    let (open, close) = (text.find('(')?, text.rfind(')')?);
    if close <= open {
        return None;
    }
    let mut f = c.token_fix(
        a,
        b,
        format!("`uvm_info(get_type_name(), $sformatf{}, UVM_LOW)", &text[open..=close]),
        "Report through `uvm_info",
    )?;
    f.safety = crate::fix::FixSafety::Unsafe;
    Some(f)
}
