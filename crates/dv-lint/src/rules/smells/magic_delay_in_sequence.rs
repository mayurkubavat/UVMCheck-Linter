//! SV-SML-002 (MagicDelayInSequence): Disallow hardcoded physical delays
//! (#delay) inside uvm_sequence.

use crate::support::class_index::Classes;
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn magic_delay_in_sequence(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = Classes::new(c);
    for n in c.nodes(TimedStmt) {
        let Some(tc) = c.tree.child(n, TimingControl) else {
            continue;
        };
        if c.keyword(tc).map(|t| c.text_of(t)) != Some("#") {
            continue;
        }
        let Some(ci) = classes.enclosing(c, n) else { continue };
        if !is_sequence(&classes, ci) {
            continue;
        }
        let Some(expr) = c.tree.children(tc).next() else {
            continue;
        };
        let zero = c.kind(expr) == Literal && literal_is_zero(&c.trimmed(expr));
        if zero {
            continue;
        }
        // slang's `toString()` of the delay expression.
        let text = c.text(expr);
        report(
            s,
            units.times(c, n),
            "SV-SML-002",
            "MagicDelayInSequence",
            Severity::Warning,
            format!(
                "Hardcoded physical time delay '#{text}' inside uvm_sequence. \
                 Sequences must be clock-frequency agnostic to ensure portability across emulation, FPGA prototyping, and ASIC frequencies. \
                 Synchronize using clocking blocks ('repeat(N) @(vif.cb)') or protocol handshakes ('wait(vif.ready)') instead."
            ),
            start(c, n),
        );
    }
}

/// An integer literal whose value is 0 (time literals such as `0ns` are
/// another kind in slang).
fn literal_is_zero(t: &str) -> bool {
    let t = t.replace('_', "");
    if t.chars().all(|ch| ch.is_ascii_digit()) {
        return t.chars().all(|ch| ch == '0');
    }
    if let Some(p) = t.find('\'') {
        let digits: String = t[p + 1..].chars().skip_while(|ch| matches!(ch, 's' | 'S')).skip(1).collect();
        return !digits.is_empty() && digits.chars().all(|ch| ch == '0');
    }
    false
}

/// `UvmModel::isUvmSequence`: the class derives from uvm_sequence_base or
/// uvm_sequence (by name).
fn is_sequence(classes: &Classes, i: usize) -> bool {
    classes
        .chain(i)
        .iter()
        .any(|(n, _)| matches!(n.as_str(), "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence"))
}
