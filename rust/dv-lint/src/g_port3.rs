//! Builder slice N6.4 ports, part 3: the configurable style engine
//! (SV-STY-100, its findings carrying the sub-rule ids SV-STY-101..106)
//! with uvmcheck's default style configuration. Messages are verbatim.

use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

use crate::g_style::{at, end_and_label, end_loc, routine_name, routines};
use crate::syntax::Cst;
use crate::Check::{Mixed, Syntax};
use crate::{Rule, Severity, Sink};
use dv_model::Design;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-STY-100", name: "ConfigurableStyleEngine", check: Syntax(style_engine) },
        Rule { id: "SV-NAM-100", name: "ConfigurableNamingEngine", check: Mixed(naming_engine) },
    ]
}

/// The ids SV-STY-100 reports under.
pub(crate) const STYLE_SUBS: &[(&str, &str)] = &[
    ("SV-STY-101", "TabCharactersProhibited"),
    ("SV-STY-102", "TrailingWhitespace"),
    ("SV-STY-103", "MaxLineLengthExceeded"),
    ("SV-STY-104", "MandatoryMatchingEndLabel"),
];

/// `StyleConfiguration` defaults.
const MAX_LINE_LENGTH: usize = 120;

fn style_engine(c: &Cst, s: &mut Sink) {
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

// ---- SV-NAM-100 ----

/// The ids SV-NAM-100 reports under (the default naming rules; SV-NAM-104
/// is off by default and SV-NAM-103 has no check).
pub(crate) const NAMING_SUBS: &[(&str, &str)] = &[
    ("SV-NAM-101", "PackageNaming"),
    ("SV-NAM-102", "InterfaceNaming"),
    ("SV-NAM-105", "CovergroupNaming"),
    ("SV-NAM-106", "CoverpointNaming"),
    ("SV-NAM-107", "CrossNaming"),
    ("SV-NAM-109", "ParameterNaming"),
    ("SV-NAM-110", "TypedefEnumNaming"),
    ("SV-NAM-111", "TypedefNaming"),
    ("SV-NAM-112", "ConstraintNaming"),
    ("UVM-NAM-101", "UvmDriverNaming"),
    ("UVM-NAM-102", "UvmMonitorNaming"),
    ("UVM-NAM-103", "UvmSequencerNaming"),
    ("UVM-NAM-104", "UvmSequenceNaming"),
    ("UVM-NAM-105", "UvmSequenceItemNaming"),
    ("UVM-NAM-106", "UvmAgentNaming"),
    ("UVM-NAM-107", "UvmEnvNaming"),
    ("UVM-NAM-108", "UvmTestNaming"),
    ("UVM-NAM-109", "UvmScoreboardNaming"),
];

#[derive(Clone, Copy, PartialEq)]
enum Casing {
    None,
    Snake,
    AllCaps,
}

/// A `^...$` pattern of the default rules: an optional fixed start, a body
/// of identifier characters, and one of some fixed ends.
struct Pattern {
    text: &'static str,
    lower_only: bool,
    starts: &'static [&'static str],
    ends: &'static [&'static str],
}

/// `NamingTargetSpec` (the defaults of `loadDefaultRules`).
struct Spec {
    id: &'static str,
    kind: &'static str,
    pattern: Option<Pattern>,
    prefix: &'static str,
    suffix: &'static str,
    casing: Casing,
    message: &'static str,
}

const fn pat(text: &'static str, lower_only: bool, starts: &'static [&'static str], ends: &'static [&'static str]) -> Option<Pattern> {
    Some(Pattern { text, lower_only, starts, ends })
}

fn spec(kind: &str) -> Option<Spec> {
    use Casing::*;
    let s = |id, kind, pattern, prefix, suffix, casing, message| Spec { id, kind, pattern, prefix, suffix, casing, message };
    Some(match kind {
        "Package" => s("SV-NAM-101", "Package", pat("^[a-z0-9_]+_pkg$", true, &[], &["_pkg"]), "", "_pkg", Snake, "Packages should end with '_pkg' for clean namespace identification."),
        "Interface" => s("SV-NAM-102", "Interface", pat("^[a-z0-9_]+_if$", true, &[], &["_if"]), "", "_if", Snake, "Interfaces should end with '_if' suffix."),
        "UvmDriver" => s("UVM-NAM-101", "UvmDriver", pat("^[a-zA-Z0-9_]+_(driver|drv)$", false, &[], &["_driver", "_drv"]), "", "", None, "UVM driver components must end with '_driver' or '_drv'."),
        "UvmMonitor" => s("UVM-NAM-102", "UvmMonitor", pat("^[a-zA-Z0-9_]+_(monitor|mon)$", false, &[], &["_monitor", "_mon"]), "", "", None, "UVM monitor components must end with '_monitor' or '_mon'."),
        "UvmSequencer" => s("UVM-NAM-103", "UvmSequencer", pat("^[a-zA-Z0-9_]+_(sequencer|sqr)$", false, &[], &["_sequencer", "_sqr"]), "", "", None, "UVM sequencers must end with '_sequencer' or '_sqr'."),
        "UvmSequence" => s("UVM-NAM-104", "UvmSequence", pat("^[a-zA-Z0-9_]+_(seq|sequence)$", false, &[], &["_seq", "_sequence"]), "", "", None, "UVM sequence classes must end with '_seq' or '_sequence'."),
        "UvmSequenceItem" => s("UVM-NAM-105", "UvmSequenceItem", pat("^[a-zA-Z0-9_]+_(item|seq_item|trans|transaction)$", false, &[], &["_item", "_seq_item", "_trans", "_transaction"]), "", "", None, "UVM transaction/sequence items must end with '_item' or '_seq_item'."),
        "UvmAgent" => s("UVM-NAM-106", "UvmAgent", pat("^[a-zA-Z0-9_]+_agent$", false, &[], &["_agent"]), "", "_agent", None, "UVM agents must end with '_agent' suffix."),
        "UvmEnv" => s("UVM-NAM-107", "UvmEnv", pat("^[a-zA-Z0-9_]+_env$", false, &[], &["_env"]), "", "_env", None, "UVM environments must end with '_env' suffix."),
        "UvmTest" => s("UVM-NAM-108", "UvmTest", pat("^[a-zA-Z0-9_]+_test$", false, &[], &["_test"]), "", "_test", None, "UVM test classes must end with '_test' suffix."),
        "UvmScoreboard" => s("UVM-NAM-109", "UvmScoreboard", pat("^[a-zA-Z0-9_]+_(scoreboard|scb)$", false, &[], &["_scoreboard", "_scb"]), "", "", None, "UVM scoreboards must end with '_scoreboard' or '_scb'."),
        "Covergroup" => s("SV-NAM-105", "Covergroup", pat("^[a-zA-Z0-9_]+_cg$", false, &[], &["_cg"]), "", "_cg", None, "Covergroup declarations must end with '_cg' suffix."),
        "Coverpoint" => s("SV-NAM-106", "Coverpoint", pat("^cp_[a-zA-Z0-9_]+$", false, &["cp_"], &[]), "cp_", "", None, "Coverpoints should start with 'cp_' prefix."),
        "Cross" => s("SV-NAM-107", "Cross", pat("^(cross|cx)_[a-zA-Z0-9_]+$", false, &["cross_", "cx_"], &[]), "", "", None, "Cross coverages should start with 'cross_' or 'cx_' prefix."),
        "Parameter" => s("SV-NAM-109", "Parameter", Option::None, "", "", AllCaps, "Parameters and constants should follow ALL_CAPS conventions."),
        "TypedefEnum" => s("SV-NAM-110", "TypedefEnum", pat("^[a-zA-Z0-9_]+_e$", false, &[], &["_e"]), "", "_e", None, "Enum typedefs should end with '_e' suffix."),
        "Typedef" => s("SV-NAM-111", "Typedef", pat("^[a-zA-Z0-9_]+_t$", false, &[], &["_t"]), "", "_t", None, "User-defined typedefs should end with '_t' suffix."),
        "Constraint" => s("SV-NAM-112", "Constraint", pat("^[a-zA-Z0-9_]+_c$", false, &[], &["_c"]), "", "_c", None, "Class constraint blocks should end with '_c' suffix."),
        _ => return Option::None,
    })
}

impl Pattern {
    /// `std::regex_match` of these patterns: a fixed start or end around a
    /// body of at least one identifier character.
    fn matches(&self, name: &str) -> bool {
        let ok = |s: &str| !s.is_empty() && s.bytes().all(|b| b == b'_' || b.is_ascii_digit() || b.is_ascii_lowercase() || (!self.lower_only && b.is_ascii_uppercase()));
        if !self.starts.is_empty() {
            return self.starts.iter().any(|p| name.strip_prefix(p).is_some_and(ok));
        }
        self.ends.iter().any(|e| name.strip_suffix(e).is_some_and(ok))
    }
}

fn casing_ok(c: Casing, name: &str) -> bool {
    match c {
        Casing::None => true,
        Casing::Snake => !name.is_empty() && !name.bytes().any(|b| b.is_ascii_uppercase()),
        Casing::AllCaps => !name.is_empty() && !name.bytes().any(|b| b.is_ascii_lowercase()),
    }
}

/// `NamingEngine::validate`: the violation reason, or None when `name` fits.
fn validate(kind: &str, name: &str) -> Option<(Spec, String)> {
    let sp = spec(kind)?;
    let fits = sp.pattern.as_ref().is_none_or(|p| p.matches(name))
        && name.starts_with(sp.prefix)
        && name.ends_with(sp.suffix)
        && casing_ok(sp.casing, name);
    if fits {
        return None;
    }
    let mut req: Vec<String> = Vec::new();
    if !sp.prefix.is_empty() && !name.starts_with(sp.prefix) {
        req.push(format!("must begin with prefix '{}'", sp.prefix));
    }
    if !sp.suffix.is_empty() && !name.ends_with(sp.suffix) {
        req.push(format!("must end with suffix '{}'", sp.suffix));
    }
    if let Some(p) = &sp.pattern {
        req.push(format!("must match pattern '{}'", p.text));
    }
    if !casing_ok(sp.casing, name) {
        req.push(match sp.casing {
            Casing::Snake => "must follow snake_case".into(),
            _ => "must follow ALL_CAPS".into(),
        });
    }
    let reason = format!("{} identifier '{name}' does not satisfy naming convention: {}. {}", sp.kind, req.join(", "), sp.message);
    Some((sp, reason))
}

fn report(out: &mut Sink, kind: &str, rule_name: &'static str, name: &str, loc: dv_model::Loc) {
    if let Some((sp, reason)) = validate(kind, name) {
        out.add(sp.id, rule_name, Severity::Warning, reason, loc);
    }
}

/// `NamingEngine::classTargetKind` (the UVM kinds; plain classes are off).
fn class_kind(d: &Design, id: dv_model::ClassId) -> Option<&'static str> {
    let derived = |n: &str| d.class(id).name == n || d.ancestors(id).any(|a| a.name == n);
    Some(if derived("uvm_driver") {
        "UvmDriver"
    } else if derived("uvm_monitor") {
        "UvmMonitor"
    } else if derived("uvm_sequencer") {
        "UvmSequencer"
    } else if derived("uvm_sequence") || derived("uvm_sequence_base") {
        "UvmSequence"
    } else if derived("uvm_sequence_item") {
        "UvmSequenceItem"
    } else if derived("uvm_agent") {
        "UvmAgent"
    } else if derived("uvm_env") {
        "UvmEnv"
    } else if derived("uvm_test") {
        "UvmTest"
    } else if derived("uvm_scoreboard") {
        "UvmScoreboard"
    } else {
        return None;
    })
}

fn naming_engine(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Packages of the compile, the first definition of each name.
    let mut seen = std::collections::HashSet::new();
    for c in csts {
        for p in c.nodes(PackageDecl) {
            let Some(nm) = c.tree.child(p, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            let name = c.text_of(t).to_string();
            if name == "std" || !seen.insert(name.clone()) {
                continue;
            }
            report(out, "Package", "PackageNamingConvention", &name, c.loc(t));
        }
    }
    // Model classes and their parameters.
    let ix = crate::g_methodology::Index::new(csts);
    for cls in d.classes().iter().filter(|k| !k.is_parameterized) {
        if cls.loc.expansion.is_some() {
            continue;
        }
        if let Some(kind) = class_kind(d, cls.id) {
            report(out, kind, "UvmClassNamingConvention", &cls.name, cls.loc);
        }
        let Some((c, n)) = ix.decl(cls.loc) else { continue };
        if c.kind(n) != ClassDecl {
            continue;
        }
        for p in c.tree.children(n).filter(|&p| c.kind(p) == ParamDecl) {
            if c.toks(p).any(|t| c.text_of(t) == "type") {
                continue;
            }
            for x in c.tree.children(p).filter(|&x| c.kind(x) == Declarator) {
                let Some(t) = c.tree.child(x, Name).and_then(|nm| c.first_tok(nm)) else { continue };
                if c.tree.tokens[t as usize].from_macro {
                    continue;
                }
                report(out, "Parameter", "ConstantNamingConvention", c.text_of(t), c.loc(t));
            }
        }
    }
    // The syntax: covergroups, coverpoints, crosses, constraints, typedefs
    // and interfaces, not where a macro wrote the name.
    for c in csts {
        let tok = |n: NodeId| c.first_tok(n).filter(|&t| !c.tree.tokens[t as usize].from_macro);
        for g in c.nodes(CovergroupDecl) {
            let Some(t) = c.tree.child(g, Name).and_then(|nm| tok(nm)) else { continue };
            report(out, "Covergroup", "CovergroupNamingConvention", c.text_of(t), c.loc(t));
        }
        for (k, kind, rn) in [(Coverpoint, "Coverpoint", "CoverpointNamingConvention"), (Cross, "Cross", "CrossNamingConvention")] {
            for n in c.nodes(k) {
                let Some(t) = c.tree.child(n, StmtLabel).and_then(|l| tok(l)) else { continue };
                report(out, kind, rn, c.text_of(t), c.loc(t));
            }
        }
        for n in c.nodes(ConstraintDecl) {
            if c.tree.child(n, ConstraintBlock).is_none() {
                continue;
            }
            let Some(nm) = c.tree.child(n, Name) else { continue };
            let Some(t) = tok(nm) else { continue };
            report(out, "Constraint", "ConstraintNamingConvention", c.last_text(nm), c.loc(t));
        }
        for n in c.nodes(TypedefDecl) {
            let Some(t) = c.tree.child(n, Name).and_then(|nm| tok(nm)) else { continue };
            let is_enum = c.tree.child(n, DataType).and_then(|ty| c.first_tok(ty)).is_some_and(|f| c.text_of(f) == "enum");
            let (kind, rn) = if is_enum { ("TypedefEnum", "TypedefEnumNamingConvention") } else { ("Typedef", "TypedefNamingConvention") };
            report(out, kind, rn, c.text_of(t), c.loc(t));
        }
        for n in c.nodes(InterfaceDecl) {
            let Some(t) = c.tree.child(n, Name).and_then(|nm| tok(nm)) else { continue };
            report(out, "Interface", "InterfaceNamingConvention", c.text_of(t), c.loc(t));
        }
    }
}
