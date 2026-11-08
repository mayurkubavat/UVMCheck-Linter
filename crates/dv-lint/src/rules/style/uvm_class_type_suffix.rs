//! SV-NAM-005 (UvmClassTypeSuffix): Enforces standard verification naming
//! suffix conventions for classes extending UVM base components (e.g. _seq,
//! _item, _driver, _monitor, _scoreboard, _env).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::source_layout::{classes};

const CLASS_SUFFIXES: [(&str, &[&str], &str); 8] = [
    ("uvm_sequence_item", &["_item", "_tr", "_trans", "_pkt", "Item", "Transaction", "Packet", "Tr"], "'_item' or '_tr'"),
    ("uvm_sequence", &["_seq", "_sequence", "_vseq", "Seq", "Sequence", "Vseq"], "'_seq'"),
    ("uvm_driver", &["_driver", "_drv", "Driver", "Drv"], "'_driver' or '_drv'"),
    ("uvm_monitor", &["_monitor", "_mon", "Monitor", "Mon"], "'_monitor' or '_mon'"),
    ("uvm_scoreboard", &["_scoreboard", "_scb", "Scoreboard", "Scb"], "'_scoreboard' or '_scb'"),
    ("uvm_agent", &["_agent", "_agt", "Agent", "Agt"], "'_agent' or '_agt'"),
    ("uvm_env", &["_env", "Env"], "'_env'"),
    ("uvm_test", &["_test", "Test"], "'_test'"),
];

pub(crate) fn uvm_class_suffix(c: &Cst, s: &mut Sink) {
    for (n, name) in classes(c) {
        let Some(ext) = c.tree.child(n, Header).and_then(|h| c.tree.child(h, Extends)) else { continue };
        let Some(ty) = c.tree.child(ext, DataType) else { continue };
        let base = c.last_text(ty);
        if name.starts_with("uvm_") {
            continue;
        }
        let pick = CLASS_SUFFIXES.iter().find(|(b, _, _)| match *b {
            "uvm_sequence" => base.contains("uvm_sequence") && !base.contains("sequence_item"),
            b => base.contains(b),
        });
        let Some((_, suffixes, rec)) = pick else { continue };
        if suffixes.iter().any(|x| name.ends_with(x)) {
            continue;
        }
        let Some(nm) = c.tree.child(n, Name).and_then(|x| c.first_tok(x)) else { continue };
        s.add(
            "SV-NAM-005",
            "UvmClassTypeSuffix",
            Severity::Warning,
            format!(
                "Class '{name}' extends '{base}' but does not end with recommended verification suffix {rec}. Standardizing UVM class suffixes improves component discoverability and codebase readability."
            ),
            c.loc(nm),
        );
    }
}
