//! SV-NAM-100 (ConfigurableNamingEngine): Parameterizable naming convention
//! engine validating regex, prefix, suffix, and casing conventions across
//! packages, interfaces, UVM classes, covergroups, and constraints.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

#[derive(Clone, Copy, PartialEq)]
enum Casing {
    None,
    Snake,
    Pascal,
    Camel,
    AllCaps,
}

/// A naming pattern: its text, and compiled for `std::regex_match`
/// ([`crate::pattern`]). None when it does not compile: uvmcheck then skips
/// the pattern check (the other requirements still apply).
#[derive(Clone)]
struct Pattern {
    text: &'static str,
    re: Option<crate::pattern::Pattern>,
}

impl Pattern {
    fn matches(&self, name: &str) -> bool {
        self.re.as_ref().is_none_or(|re| re.is_full_match(name))
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

/// `text` compiled, once per pattern for the run.
fn pat(text: &str) -> Option<Pattern> {
    static MADE: std::sync::Mutex<Vec<Pattern>> = std::sync::Mutex::new(Vec::new());
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = made.iter().find(|p| p.text == text) {
        return Some(p.clone());
    }
    let p = Pattern {
        text: Box::leak(text.to_string().into_boxed_str()),
        re: crate::pattern::Pattern::new(text, false).ok(),
    };
    made.push(p.clone());
    Some(p)
}

/// The spec of `kind` under the project config; None when there is none.
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
        } else {
            sp.pattern = pat(&p);
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
/// uvmcheck but checks nothing there either), and a pattern that compiles.
pub(crate) fn target_supported(kind: &str, pattern: &str) -> bool {
    if kind == "VirtualInterface" {
        return true;
    }
    default_spec(kind).is_some() && (pattern.is_empty() || crate::pattern::Pattern::new(pattern, false).is_ok())
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
            pat("^[a-z0-9_]+_pkg$"),
            "",
            "_pkg",
            Snake,
            "Packages should end with '_pkg' for clean namespace identification.",
        ),
        "Interface" => s(
            "SV-NAM-102",
            "Interface",
            pat("^[a-z0-9_]+_if$"),
            "",
            "_if",
            Snake,
            "Interfaces should end with '_if' suffix.",
        ),
        "UvmDriver" => s(
            "UVM-NAM-101",
            "UvmDriver",
            pat("^[a-zA-Z0-9_]+_(driver|drv)$"),
            "",
            "",
            None,
            "UVM driver components must end with '_driver' or '_drv'.",
        ),
        "UvmMonitor" => s(
            "UVM-NAM-102",
            "UvmMonitor",
            pat("^[a-zA-Z0-9_]+_(monitor|mon)$"),
            "",
            "",
            None,
            "UVM monitor components must end with '_monitor' or '_mon'.",
        ),
        "UvmSequencer" => s(
            "UVM-NAM-103",
            "UvmSequencer",
            pat("^[a-zA-Z0-9_]+_(sequencer|sqr)$"),
            "",
            "",
            None,
            "UVM sequencers must end with '_sequencer' or '_sqr'.",
        ),
        "UvmSequence" => s(
            "UVM-NAM-104",
            "UvmSequence",
            pat("^[a-zA-Z0-9_]+_(seq|sequence)$"),
            "",
            "",
            None,
            "UVM sequence classes must end with '_seq' or '_sequence'.",
        ),
        "UvmSequenceItem" => s(
            "UVM-NAM-105",
            "UvmSequenceItem",
            pat("^[a-zA-Z0-9_]+_(item|seq_item|trans|transaction)$"),
            "",
            "",
            None,
            "UVM transaction/sequence items must end with '_item' or '_seq_item'.",
        ),
        "UvmAgent" => s(
            "UVM-NAM-106",
            "UvmAgent",
            pat("^[a-zA-Z0-9_]+_agent$"),
            "",
            "_agent",
            None,
            "UVM agents must end with '_agent' suffix.",
        ),
        "UvmEnv" => s(
            "UVM-NAM-107",
            "UvmEnv",
            pat("^[a-zA-Z0-9_]+_env$"),
            "",
            "_env",
            None,
            "UVM environments must end with '_env' suffix.",
        ),
        "UvmTest" => s(
            "UVM-NAM-108",
            "UvmTest",
            pat("^[a-zA-Z0-9_]+_test$"),
            "",
            "_test",
            None,
            "UVM test classes must end with '_test' suffix.",
        ),
        "UvmScoreboard" => s(
            "UVM-NAM-109",
            "UvmScoreboard",
            pat("^[a-zA-Z0-9_]+_(scoreboard|scb)$"),
            "",
            "",
            None,
            "UVM scoreboards must end with '_scoreboard' or '_scb'.",
        ),
        "Covergroup" => s(
            "SV-NAM-105",
            "Covergroup",
            pat("^[a-zA-Z0-9_]+_cg$"),
            "",
            "_cg",
            None,
            "Covergroup declarations must end with '_cg' suffix.",
        ),
        "Coverpoint" => s(
            "SV-NAM-106",
            "Coverpoint",
            pat("^cp_[a-zA-Z0-9_]+$"),
            "cp_",
            "",
            None,
            "Coverpoints should start with 'cp_' prefix.",
        ),
        "Cross" => s(
            "SV-NAM-107",
            "Cross",
            pat("^(cross|cx)_[a-zA-Z0-9_]+$"),
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
            pat("^[a-zA-Z0-9_]+_e$"),
            "",
            "_e",
            None,
            "Enum typedefs should end with '_e' suffix.",
        ),
        "Typedef" => s(
            "SV-NAM-111",
            "Typedef",
            pat("^[a-zA-Z0-9_]+_t$"),
            "",
            "_t",
            None,
            "User-defined typedefs should end with '_t' suffix.",
        ),
        "Constraint" => s(
            "SV-NAM-112",
            "Constraint",
            pat("^[a-zA-Z0-9_]+_c$"),
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
pub(crate) fn class_kind(d: &Design, id: dv_model::ClassId) -> Option<&'static str> {
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

/// For `--init`: whether `kind` has an enabled convention, its pattern text
/// ("" for none), and whether `name` follows it.
pub(crate) fn convention(kind: &str, name: &str) -> Option<(&'static str, bool)> {
    let sp = spec(kind)?;
    sp.enabled.then(|| (sp.pattern.as_ref().map_or("", |p| p.text), validate(kind, name).is_none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [&str; 18] = [
        "Package",
        "Interface",
        "UvmDriver",
        "UvmMonitor",
        "UvmSequencer",
        "UvmSequence",
        "UvmSequenceItem",
        "UvmAgent",
        "UvmEnv",
        "UvmTest",
        "UvmScoreboard",
        "Covergroup",
        "Coverpoint",
        "Cross",
        "Parameter",
        "TypedefEnum",
        "Typedef",
        "Constraint",
    ];

    #[test]
    fn default_patterns_compile() {
        for k in KINDS {
            let sp = default_spec(k).unwrap();
            if let Some(p) = sp.pattern {
                assert!(p.re.is_some(), "{k}: {}", p.text);
            }
        }
    }

    #[test]
    fn default_patterns_match_whole_names() {
        let m = |k: &str, n: &str| default_spec(k).unwrap().pattern.unwrap().matches(n);
        assert!(m("Package", "bus_pkg") && !m("Package", "Bus_pkg") && !m("Package", "_pkg"));
        assert!(m("UvmDriver", "Bus_drv") && m("UvmDriver", "bus_driver") && !m("UvmDriver", "bus_drv2"));
        assert!(m("Coverpoint", "cp_a") && !m("Coverpoint", "cp_") && !m("Coverpoint", "xcp_a"));
        assert!(m("Cross", "cx_ab") && m("Cross", "cross_ab") && !m("Cross", "cr_ab"));
    }
}
