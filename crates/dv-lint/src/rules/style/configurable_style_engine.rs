//! SV-STY-100 (ConfigurableStyleEngine): Parameterizable code style engine
//! enforcing tab prohibitions, trailing whitespace limits, maximum line
//! lengths, matching block end labels, and file naming.

use crate::support::source_layout::{at, end_and_label, end_loc, routine_name, routines};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// `StyleConfiguration` defaults.
const MAX_LINE_LENGTH: usize = 120;

pub(crate) fn style_engine(c: &Cst, s: &mut Sink) {
    textual(c, s);
    structural(c, s);
}

/// `inspectTextualStyle`: the buffer of the tree's first token (its root's
/// start), read line by line as `std::getline` does. A line's '\r' is
/// dropped before its length counts toward the next line's offset.
fn textual(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    let file = match t.tokens.first() {
        // A macro's token: the file of its expansion.
        Some(tok) => tok.origin.file,
        None if !t.mains.is_empty() => t.mains[0],
        None => return,
    };
    let mut text = t.sources[file as usize].to_string();
    // slang's buffer ends in a NUL.
    text.push('\0');
    let mut offset = 0usize;
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let (raw, next) = match rest.find('\n') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let b = line.as_bytes();
        if let Some(tab) = line.find('\t') {
            s.add(
                "SV-STY-101",
                "TabCharactersProhibited",
                Severity::Warning,
                format!(
                    "Literal tab character found at column {}. In SystemVerilog style guidelines, tab characters must be replaced with spaces to prevent inconsistent rendering across editors and command-line viewers.",
                    tab + 1
                ),
                at(c, file, offset + tab),
            );
        }
        if matches!(b.last(), Some(b' ' | b'\t')) {
            let ws = b.iter().rposition(|&x| x != b' ' && x != b'\t').map_or(0, |i| i + 1);
            s.add(
                "SV-STY-102",
                "TrailingWhitespace",
                Severity::Warning,
                "Line contains trailing whitespace characters at line end. Remove trailing spaces/tabs to prevent noisy version control diffs.".into(),
                at(c, file, offset + ws),
            );
        }
        if b.len() > MAX_LINE_LENGTH {
            s.add(
                "SV-STY-103",
                "MaxLineLengthExceeded",
                Severity::Warning,
                format!(
                    "Line exceeds maximum length limit of {MAX_LINE_LENGTH} characters (current: {}). Wrap long lines for readable side-by-side reviews.",
                    b.len()
                ),
                at(c, file, offset + MAX_LINE_LENGTH),
            );
        }
        offset += b.len() + 1;
        rest = next;
    }
}

/// `StructuralStyleVisitor`: end labels of classes, routines and interfaces.
fn structural(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ClassDecl) {
        let Some((end, None)) = end_and_label(c, n, &["endclass"]) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        s.add(
            "SV-STY-104",
            "MandatoryMatchingEndLabel",
            Severity::Warning,
            format!("Class '{name}' definition lacks matching end label (expected 'endclass : {name}')."),
            end_loc(c, end),
        );
    }
    for (n, _) in routines(c) {
        let Some((end, None)) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let full = routine_name(c, n);
        let name = full.rsplit("::").next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let (kind, kw) = if c.kind(n) == TaskDecl { ("Task", "endtask") } else { ("Function", "endfunction") };
        s.add(
            "SV-STY-104",
            "MandatoryMatchingEndLabel",
            Severity::Warning,
            format!("{kind} '{name}' lacks matching end label (expected '{kw} : {name}')."),
            end_loc(c, end),
        );
    }
    for n in c.nodes(InterfaceDecl) {
        let Some((end, None)) = end_and_label(c, n, &["endinterface"]) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        s.add(
            "SV-STY-104",
            "MandatoryMatchingEndLabel",
            Severity::Warning,
            format!("Interface '{name}' lacks matching end label (expected 'endinterface : {name}')."),
            end_loc(c, end),
        );
    }
}
