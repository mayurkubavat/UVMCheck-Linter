//! SV-RDB-008 (UnclearAbbreviation): Flags class, property, method and local
//! names containing a vowel-less word of four or more letters (trnsfr, chnnl,
//! rgstr) that is not a common DV short form (clk, cfg, ctrl, jtag, ...).

use dv_model::{Design, Loc};
use sv_syntax::NodeId;
use crate::support::trivia::ident_words;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::class_docs::{Classes, class_methods, name_tok, properties};

fn is_unclear_abbreviation(word: &str) -> bool {
    const LEXICON: &[&str] = &[
        "clk", "rst", "cfg", "cmd", "pkt", "txn", "req", "rsp", "vif", "drv", "mon", "sqr", "scb", "env", "agt", "ral",
        "csr", "irq", "crc", "fsm", "spi", "ddr", "dll", "pll", "phy", "rtl", "hdl", "dpm", "xfr", "msg", "ptr", "cnt",
        "len", "buf", "src", "dst", "mst", "slv", "lst", "pwr", "mmcm", "bfm", "dmn", "dsp", "jtag", "gpr", "rcv",
        "trn", "sts", "ctl", "ctrl", "ctx", "hw", "sw",
    ];
    if word.len() < 4 || word.bytes().any(|b| b"aeiouy".contains(&b)) {
        return false;
    }
    if LEXICON.contains(&word) {
        return false;
    }
    !(word.ends_with('s') && LEXICON.contains(&&word[..word.len() - 1]))
}

/// Arguments of a routine: (name, name token).
fn arguments(c: &Cst, routine: NodeId) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let Some(pl) = c.tree.child(routine, PortList) else { return out };
    for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
        for dd in c.tree.children(p).filter(|&x| c.kind(x) == Declarator) {
            let Some(nm) = c.tree.child(dd, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            out.push((c.tree.bare_text(nm), t));
        }
    }
    out
}

pub(crate) fn unclear_abbreviation(_: &Design, csts: &[Cst], s: &mut Sink) {
    let classes = Classes::new(csts);
    let check = |s: &mut Sink, name: &str, loc: Loc| {
        if let Some(w) = ident_words(name).into_iter().find(|w| is_unclear_abbreviation(w)) {
            s.add(
                "SV-RDB-008",
                "UnclearAbbreviation",
                Severity::Info,
                format!("'{name}' abbreviates by dropping vowels ('{w}'). Spell the word out, or use the team's agreed short form."),
                loc,
            );
        }
    };
    for (_, k) in classes.user() {
        let c = &csts[k.ix];
        if let Some(t) = name_tok(c, k.node) {
            check(s, &k.name, c.loc(t));
        }
        for (p, t) in properties(c, k.node) {
            check(s, &p, c.loc(t));
        }
        for m in class_methods(csts, k) {
            let mc = &csts[m.ix];
            // The method symbol is located at its name in the body's
            // declaration (the out-of-block one for an extern method).
            let Some(t) = mc.tree.child(m.node, Name).and_then(|x| mc.tree.token_bounds(x)).map(|(_, b)| b) else {
                continue;
            };
            check(s, &m.name, mc.loc(t));
            for (a, t) in arguments(mc, m.node) {
                check(s, &a, mc.loc(t));
            }
        }
    }
}
