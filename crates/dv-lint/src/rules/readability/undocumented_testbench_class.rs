//! SV-RDB-010 (UndocumentedTestbenchClass): Flags UVM components and sequences
//! without a comment of at least min_words (default 10) words of prose right
//! above the class.

use crate::support::class_docs::{name_tok, own_comments, Classes, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

/// A class's or method's first token is from a macro or a uvm_* file.
fn library_tok(c: &Cst, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    tok.from_macro
        || c.tree.files[tok.origin.file as usize].file_name().is_some_and(|n| n.to_string_lossy().starts_with("uvm_"))
}

fn prose_words(comments: &[(u32, String)]) -> usize {
    let mut words = 0;
    for (_, text) in comments {
        for line in text.split('\n') {
            let line = line.to_ascii_lowercase();
            if line.contains("copyright") || line.contains("license") || line.contains("spdx") {
                continue;
            }
            let mut in_word = false;
            for ch in line.bytes() {
                let letter = ch.is_ascii_alphabetic();
                if letter && !in_word {
                    words += 1;
                }
                in_word = letter;
            }
        }
    }
    words
}

pub(crate) fn undocumented_testbench_class(_: &Design, csts: &[Cst], s: &mut Sink) {
    const MIN_WORDS: usize = 10;
    let classes = Classes::new(csts);
    for (_, k) in classes.user() {
        if !k.role.is_component() && k.role != Role::Sequence {
            continue;
        }
        if k.generic {
            continue;
        }
        let c = &csts[k.ix];
        let Some(first) = c.first_tok(k.node) else { continue };
        if library_tok(c, first) {
            continue;
        }
        if prose_words(&own_comments(c, first)) >= MIN_WORDS {
            continue;
        }
        let Some(nt) = name_tok(c, k.node) else { continue };
        s.add(
            "SV-RDB-010",
            "UndocumentedTestbenchClass",
            Severity::Info,
            format!(
                "{} '{}' has no description. Two sentences above the class on what it models, drives \
                 or checks save every reader a trip through the code.",
                k.role.name(),
                k.name
            ),
            c.loc(nt),
        );
    }
}
