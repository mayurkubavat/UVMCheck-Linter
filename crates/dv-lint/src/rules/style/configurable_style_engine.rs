//! SV-STY-100 (ConfigurableStyleEngine): Parameterizable code style engine
//! enforcing tab prohibitions, trailing whitespace limits, maximum line
//! lengths, matching block end labels, and file naming.

use crate::support::source_layout::{at, end_and_label, end_loc, routine_name, routines};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// The `style` block of the project configuration, with
/// `StyleConfiguration`'s defaults.
struct Style {
    max_line_length: usize,
    allow_tabs: bool,
    allow_trailing_whitespace: bool,
    mandatory_matching_end_label: bool,
    /// `require_file_header` with its pattern's alternatives, lower case.
    header: Option<(String, Vec<String>)>,
    file_matches_class_name: bool,
}

impl Style {
    fn get() -> Style {
        use crate::options::{style_bool, style_int, style_str};
        let pattern = style_str("file_header_pattern", "");
        let header = (style_bool("require_file_header", false) && !pattern.is_empty())
            .then(|| crate::options::header_alternatives(&pattern).map(|alts| (pattern, alts)))
            .flatten();
        Style {
            max_line_length: style_int("max_line_length", 120).max(0) as usize,
            allow_tabs: style_bool("allow_tabs", false),
            allow_trailing_whitespace: style_bool("allow_trailing_whitespace", false),
            mandatory_matching_end_label: style_bool("mandatory_matching_end_label", true),
            header,
            file_matches_class_name: style_bool("file_matches_class_name", false),
        }
    }
}

pub(crate) fn style_engine(c: &Cst, s: &mut Sink) {
    let style = Style::get();
    textual(c, s, &style);
    structural(c, s, &style);
}

/// `inspectTextualStyle`: the buffer of the tree's first token (its root's
/// start), read line by line as `std::getline` does. A line's '\r' is
/// dropped before its length counts toward the next line's offset.
fn textual(c: &Cst, s: &mut Sink, style: &Style) {
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
    let mut line_num = 1usize;
    let mut found_header = false;
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let (raw, next) = match rest.find('\n') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let b = line.as_bytes();
        if let Some(tab) = line.find('\t').filter(|_| !style.allow_tabs) {
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
        if !style.allow_trailing_whitespace && matches!(b.last(), Some(b' ' | b'\t')) {
            let ws = b.iter().rposition(|&x| x != b' ' && x != b'\t').map_or(0, |i| i + 1);
            s.add(
                "SV-STY-102",
                "TrailingWhitespace",
                Severity::Warning,
                "Line contains trailing whitespace characters at line end. Remove trailing spaces/tabs to prevent noisy version control diffs.".into(),
                at(c, file, offset + ws),
            );
        }
        let max = style.max_line_length;
        if max > 0 && b.len() > max {
            s.add(
                "SV-STY-103",
                "MaxLineLengthExceeded",
                Severity::Warning,
                format!(
                    "Line exceeds maximum length limit of {max} characters (current: {}). Wrap long lines for readable side-by-side reviews.",
                    b.len()
                ),
                at(c, file, offset + max),
            );
        }
        // The header is looked for in the first 20 lines.
        if let Some((_, alts)) = &style.header {
            if line_num <= 20 && alts.iter().any(|a| line.to_lowercase().contains(a.as_str())) {
                found_header = true;
            }
        }
        offset += b.len() + 1;
        line_num += 1;
        rest = next;
    }
    if let Some((pattern, _)) = &style.header {
        if !found_header {
            let path = t.files.get(file as usize).map(|p| p.display().to_string()).unwrap_or_default();
            s.add(
                "SV-STY-106",
                "MissingFileHeader",
                Severity::Warning,
                format!(
                    "File '{path}' lacks required standard corporate copyright/license header pattern ('{pattern}')."
                ),
                at(c, file, 0),
            );
        }
    }
}

/// `StructuralStyleVisitor`: end labels of classes, routines and interfaces.
fn structural(c: &Cst, s: &mut Sink, style: &Style) {
    if style.file_matches_class_name {
        for n in c.nodes(ClassDecl) {
            let Some(nm) = c.tree.child(n, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            let file = c.tree.tokens[t as usize].written.file;
            let Some(p) = c.tree.files.get(file as usize) else { continue };
            let class = c.tree.bare_text(nm);
            if p.file_stem().is_some_and(|s| s.to_string_lossy() != class) {
                let fname = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                s.add(
                    "SV-STY-105",
                    "FileMatchesClassName",
                    Severity::Warning,
                    format!(
                        "File name '{fname}' does not match class name '{class}'. SystemVerilog style guidelines recommend naming files after the primary class declared within them."
                    ),
                    c.loc(t),
                );
            }
        }
    }
    if !style.mandatory_matching_end_label {
        return;
    }
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
