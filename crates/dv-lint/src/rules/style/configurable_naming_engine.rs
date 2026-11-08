//! SV-NAM-100 (ConfigurableNamingEngine): Parameterizable naming convention
//! engine validating regex, prefix, suffix, and casing conventions across
//! packages, interfaces, UVM classes, covergroups, and constraints.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

#[derive(Clone, Copy, PartialEq)]
enum Casing {
    None,
    Snake,
    Pascal,
    Camel,
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

impl Pattern {
    /// `std::regex_match` of these patterns: a fixed start or end around a
    /// body of at least one identifier character.
    fn matches(&self, name: &str) -> bool {
        let ok = |s: &str| {
            !s.is_empty()
                && s.bytes().all(|b| {
                    b == b'_'
                        || b.is_ascii_digit()
                        || b.is_ascii_lowercase()
                        || (!self.lower_only && b.is_ascii_uppercase())
                })
        };
        if !self.starts.is_empty() {
            return self.starts.iter().any(|p| name.strip_prefix(p).is_some_and(ok));
        }
        self.ends.iter().any(|e| name.strip_suffix(e).is_some_and(ok))
    }
}

/// `NamingTargetSpec`: the defaults of `loadDefaultRules`, with the project
/// config's `naming.<Target>` settings over them.
struct Spec {
    id: &'static str,
    kind: &'static str,
    pattern: Option<Pattern>,
    prefix: String,
    suffix: String,
    casing: Casing,
    message: &'static str,
    severity: Severity,
    enabled: bool,
}

const fn pat(
    text: &'static str,
    lower_only: bool,
    starts: &'static [&'static str],
    ends: &'static [&'static str],
) -> Option<Pattern> {
    Some(Pattern { text, lower_only, starts, ends })
}

/// The spec of `kind` under the project config; None when there is none or
/// it cannot be checked here (a pattern the config replaces with another
/// regex: no regex engine here, and the config reports it as not applied).
fn spec(kind: &str) -> Option<Spec> {
    let mut sp = default_spec(kind)?;
    let Some(o) = crate::options::naming(kind) else { return Some(sp) };
    if let Some(p) = o.prefix {
        sp.prefix = p;
    }
    if let Some(s) = o.suffix {
        sp.suffix = s;
    }
    if let Some(p) = o.pattern {
        if p.is_empty() {
            sp.pattern = Option::None;
        } else if sp.pattern.as_ref().is_none_or(|d| d.text != p) {
            return Option::None;
        }
    }
    match o.casing.as_deref() {
        Some("snake_case") => sp.casing = Casing::Snake,
        Some("PascalCase") => sp.casing = Casing::Pascal,
        Some("camelCase") => sp.casing = Casing::Camel,
        Some("ALL_CAPS") => sp.casing = Casing::AllCaps,
        _ => {}
    }
    match o.severity.as_deref().map(str::to_ascii_uppercase).as_deref() {
        Some("ERROR") => sp.severity = Severity::Error,
        Some("WARNING") => sp.severity = Severity::Warning,
        Some("INFO") => sp.severity = Severity::Info,
        _ => {}
    }
    if let Some(e) = o.enabled {
        sp.enabled = e;
    }
    Some(sp)
}

/// Whether `naming.<kind>` (with `pattern`, empty when not set) can be
/// checked here: a target the engine knows (VirtualInterface is known to
/// uvmcheck but checks nothing there either), and its default pattern or
/// none.
pub(crate) fn target_supported(kind: &str, pattern: &str) -> bool {
    if kind == "VirtualInterface" {
        return true;
    }
    let Some(d) = default_spec(kind) else { return false };
    pattern.is_empty() || d.pattern.is_some_and(|p| p.text == pattern)
}

fn default_spec(kind: &str) -> Option<Spec> {
    use Casing::*;
    let s = |id, kind, pattern, prefix: &str, suffix: &str, casing, message| Spec {
        id,
        kind,
        pattern,
        prefix: prefix.to_string(),
        suffix: suffix.to_string(),
        casing,
        message,
        severity: Severity::Warning,
        enabled: true,
    };
    Some(match kind {
        "Package" => s(
            "SV-NAM-101",
            "Package",
            pat("^[a-z0-9_]+_pkg$", true, &[], &["_pkg"]),
            "",
            "_pkg",
            Snake,
            "Packages should end with '_pkg' for clean namespace identification.",
        ),
        "Interface" => s(
            "SV-NAM-102",
            "Interface",
            pat("^[a-z0-9_]+_if$", true, &[], &["_if"]),
            "",
            "_if",
            Snake,
            "Interfaces should end with '_if' suffix.",
        ),
        "UvmDriver" => s(
            "UVM-NAM-101",
            "UvmDriver",
            pat("^[a-zA-Z0-9_]+_(driver|drv)$", false, &[], &["_driver", "_drv"]),
            "",
            "",
            None,
            "UVM driver components must end with '_driver' or '_drv'.",
        ),
        "UvmMonitor" => s(
            "UVM-NAM-102",
            "UvmMonitor",
            pat("^[a-zA-Z0-9_]+_(monitor|mon)$", false, &[], &["_monitor", "_mon"]),
            "",
            "",
            None,
            "UVM monitor components must end with '_monitor' or '_mon'.",
        ),
        "UvmSequencer" => s(
            "UVM-NAM-103",
            "UvmSequencer",
            pat("^[a-zA-Z0-9_]+_(sequencer|sqr)$", false, &[], &["_sequencer", "_sqr"]),
            "",
            "",
            None,
            "UVM sequencers must end with '_sequencer' or '_sqr'.",
        ),
        "UvmSequence" => s(
            "UVM-NAM-104",
            "UvmSequence",
            pat("^[a-zA-Z0-9_]+_(seq|sequence)$", false, &[], &["_seq", "_sequence"]),
            "",
            "",
            None,
            "UVM sequence classes must end with '_seq' or '_sequence'.",
        ),
        "UvmSequenceItem" => s(
            "UVM-NAM-105",
            "UvmSequenceItem",
            pat(
                "^[a-zA-Z0-9_]+_(item|seq_item|trans|transaction)$",
                false,
                &[],
                &["_item", "_seq_item", "_trans", "_transaction"],
            ),
            "",
            "",
            None,
            "UVM transaction/sequence items must end with '_item' or '_seq_item'.",
        ),
        "UvmAgent" => s(
            "UVM-NAM-106",
            "UvmAgent",
            pat("^[a-zA-Z0-9_]+_agent$", false, &[], &["_agent"]),
            "",
            "_agent",
            None,
            "UVM agents must end with '_agent' suffix.",
        ),
        "UvmEnv" => s(
            "UVM-NAM-107",
            "UvmEnv",
            pat("^[a-zA-Z0-9_]+_env$", false, &[], &["_env"]),
            "",
            "_env",
            None,
            "UVM environments must end with '_env' suffix.",
        ),
        "UvmTest" => s(
            "UVM-NAM-108",
            "UvmTest",
            pat("^[a-zA-Z0-9_]+_test$", false, &[], &["_test"]),
            "",
            "_test",
            None,
            "UVM test classes must end with '_test' suffix.",
        ),
        "UvmScoreboard" => s(
            "UVM-NAM-109",
            "UvmScoreboard",
            pat("^[a-zA-Z0-9_]+_(scoreboard|scb)$", false, &[], &["_scoreboard", "_scb"]),
            "",
            "",
            None,
            "UVM scoreboards must end with '_scoreboard' or '_scb'.",
        ),
        "Covergroup" => s(
            "SV-NAM-105",
            "Covergroup",
            pat("^[a-zA-Z0-9_]+_cg$", false, &[], &["_cg"]),
            "",
            "_cg",
            None,
            "Covergroup declarations must end with '_cg' suffix.",
        ),
        "Coverpoint" => s(
            "SV-NAM-106",
            "Coverpoint",
            pat("^cp_[a-zA-Z0-9_]+$", false, &["cp_"], &[]),
            "cp_",
            "",
            None,
            "Coverpoints should start with 'cp_' prefix.",
        ),
        "Cross" => s(
            "SV-NAM-107",
            "Cross",
            pat("^(cross|cx)_[a-zA-Z0-9_]+$", false, &["cross_", "cx_"], &[]),
            "",
            "",
            None,
            "Cross coverages should start with 'cross_' or 'cx_' prefix.",
        ),
        "Parameter" => s(
            "SV-NAM-109",
            "Parameter",
            Option::None,
            "",
            "",
            AllCaps,
            "Parameters and constants should follow ALL_CAPS conventions.",
        ),
        "TypedefEnum" => s(
            "SV-NAM-110",
            "TypedefEnum",
            pat("^[a-zA-Z0-9_]+_e$", false, &[], &["_e"]),
            "",
            "_e",
            None,
            "Enum typedefs should end with '_e' suffix.",
        ),
        "Typedef" => s(
            "SV-NAM-111",
            "Typedef",
            pat("^[a-zA-Z0-9_]+_t$", false, &[], &["_t"]),
            "",
            "_t",
            None,
            "User-defined typedefs should end with '_t' suffix.",
        ),
        "Constraint" => s(
            "SV-NAM-112",
            "Constraint",
            pat("^[a-zA-Z0-9_]+_c$", false, &[], &["_c"]),
            "",
            "_c",
            None,
            "Class constraint blocks should end with '_c' suffix.",
        ),
        // Off by default; the config can turn it on.
        "Class" => Spec {
            enabled: false,
            ..s(
                "SV-NAM-104",
                "Class",
                Option::None,
                "",
                "",
                Pascal,
                "General classes should follow PascalCase naming conventions.",
            )
        },
        _ => return Option::None,
    })
}

/// `isSnakeCase`, `isPascalCase`, `isCamelCase`, `isAllCaps`.
fn casing_ok(c: Casing, name: &str) -> bool {
    let first = name.bytes().next();
    match c {
        Casing::None => true,
        Casing::Snake => !name.is_empty() && !name.bytes().any(|b| b.is_ascii_uppercase()),
        Casing::Pascal => first.is_some_and(|b| b.is_ascii_uppercase()) && !name.contains('_'),
        Casing::Camel => first.is_some_and(|b| b.is_ascii_lowercase()) && !name.contains('_'),
        Casing::AllCaps => !name.is_empty() && !name.bytes().any(|b| b.is_ascii_lowercase()),
    }
}

/// `NamingEngine::validate`: the violation reason, or None when `name` fits.
fn validate(kind: &str, name: &str) -> Option<(Spec, String)> {
    let sp = spec(kind)?;
    if !sp.enabled {
        return None;
    }
    let fits = sp.pattern.as_ref().is_none_or(|p| p.matches(name))
        && name.starts_with(sp.prefix.as_str())
        && name.ends_with(sp.suffix.as_str())
        && casing_ok(sp.casing, name);
    if fits {
        return None;
    }
    let mut req: Vec<String> = Vec::new();
    if !sp.prefix.is_empty() && !name.starts_with(sp.prefix.as_str()) {
        req.push(format!("must begin with prefix '{}'", sp.prefix));
    }
    if !sp.suffix.is_empty() && !name.ends_with(sp.suffix.as_str()) {
        req.push(format!("must end with suffix '{}'", sp.suffix));
    }
    if let Some(p) = &sp.pattern {
        req.push(format!("must match pattern '{}'", p.text));
    }
    // camelCase adds no requirement to the message, as in uvmcheck.
    if !casing_ok(sp.casing, name) {
        match sp.casing {
            Casing::Snake => req.push("must follow snake_case".into()),
            Casing::Pascal => req.push("must follow PascalCase".into()),
            Casing::AllCaps => req.push("must follow ALL_CAPS".into()),
            Casing::Camel | Casing::None => {}
        }
    }
    let reason = format!(
        "{} identifier '{name}' does not satisfy naming convention: {}. {}",
        sp.kind,
        req.join(", "),
        sp.message
    );
    Some((sp, reason))
}

fn report(out: &mut Sink, kind: &str, rule_name: &'static str, name: &str, loc: dv_model::Loc) {
    if let Some((sp, reason)) = validate(kind, name) {
        out.add(sp.id, rule_name, sp.severity, reason, loc);
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
    } else if spec("Class").is_some_and(|s| s.enabled) {
        "Class"
    } else {
        return None;
    })
}

pub(crate) fn naming_engine(d: &Design, csts: &[Cst], out: &mut Sink) {
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
    let ix = crate::support::class_roles::Index::new(csts);
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
            let Some(t) = c.tree.child(g, Name).and_then(tok) else { continue };
            report(out, "Covergroup", "CovergroupNamingConvention", c.text_of(t), c.loc(t));
        }
        for (k, kind, rn) in
            [(Coverpoint, "Coverpoint", "CoverpointNamingConvention"), (Cross, "Cross", "CrossNamingConvention")]
        {
            for n in c.nodes(k) {
                let Some(t) = c.tree.child(n, StmtLabel).and_then(tok) else { continue };
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
            let Some(t) = c.tree.child(n, Name).and_then(tok) else { continue };
            let is_enum =
                c.tree.child(n, DataType).and_then(|ty| c.first_tok(ty)).is_some_and(|f| c.text_of(f) == "enum");
            let (kind, rn) = if is_enum {
                ("TypedefEnum", "TypedefEnumNamingConvention")
            } else {
                ("Typedef", "TypedefNamingConvention")
            };
            report(out, kind, rn, c.text_of(t), c.loc(t));
        }
        for n in c.nodes(InterfaceDecl) {
            let Some(t) = c.tree.child(n, Name).and_then(tok) else { continue };
            report(out, "Interface", "InterfaceNamingConvention", c.text_of(t), c.loc(t));
        }
    }
}
