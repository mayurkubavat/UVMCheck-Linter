//! Syntax rules, second batch (R3 slice 8): naming, coverage, assertions,
//! constraints, waits and legacy declarations. See [`crate::syntax`].

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-NAM-002", name: "PackageNamingConvention", check: Syntax(package_naming) },
        Rule { id: "SV-NAM-003", name: "InterfaceNamingConvention", check: Syntax(interface_naming) },
        Rule { id: "SV-TYP-010", name: "TaskWithNonVoidReturnType", check: Syntax(task_return_type) },
        Rule { id: "SVA-LBL-001", name: "UnlabeledAssertion", check: Syntax(unlabeled_assertion) },
        Rule { id: "SV-RND-014", name: "EmptyRandomizeWith", check: Syntax(empty_randomize_with) },
        Rule { id: "SV-RND-009", name: "StaticConstraint", check: Syntax(static_constraint) },
        Rule { id: "SV-COV-003", name: "CrossSameCoverpoint", check: Syntax(cross_same_coverpoint) },
        Rule { id: "SV-COV-007", name: "EmptyCovergroup", check: Syntax(empty_covergroup) },
        Rule { id: "SV-COV-008", name: "CrossWithEmptyBins", check: Syntax(cross_with_empty_bins) },
        Rule { id: "SV-COV-009", name: "CoverpointDefaultBinHazard", check: Syntax(default_bin) },
        Rule { id: "SV-COV-014", name: "CoverpointWeightZero", check: Syntax(weight_zero) },
        Rule { id: "SV-COV-015", name: "MissingIffOnCovergroupClock", check: Syntax(missing_iff) },
        Rule { id: "SV-CON-004", name: "ConstantWaitCondition", check: Syntax(constant_wait) },
        Rule { id: "SV-CON-010", name: "DisableForkWithoutScope", check: Syntax(disable_fork) },
        Rule { id: "UVM-PHS-007", name: "UvmPhaseJumpHazard", check: Syntax(phase_jump) },
        Rule { id: "SV-BAN-001", name: "LegacyRegWireInTestbench", check: Syntax(legacy_reg_wire) },
        Rule { id: "SV-DEP-006", name: "NonAnsiPortList", check: Syntax(non_ansi_ports) },
    ]
}

/// The name token of a declaration's `Name` child.
fn name_tok(c: &Cst, n: NodeId) -> Option<u32> {
    let name = c.tree.child(n, Name)?;
    c.tree.token_bounds(name).map(|(_, b)| b)
}

fn package_naming(c: &Cst, s: &mut Sink) {
    for n in c.nodes(PackageDecl) {
        let Some(t) = name_tok(c, n) else { continue };
        let name = c.text_of(t);
        if !name.is_empty() && !name.ends_with("_pkg") {
            s.add(
                "SV-NAM-002",
                "PackageNamingConvention",
                Severity::Warning,
                format!(
                    "Package '{name}' does not end with required suffix '_pkg'. \
                     SystemVerilog methodology standards require package identifiers to end with '_pkg' (e.g. '{name}_pkg') to clarify namespace scopes and prevent collision with modules."
                ),
                c.loc(t),
            );
        }
    }
}

fn interface_naming(c: &Cst, s: &mut Sink) {
    for n in c.nodes(InterfaceDecl) {
        let Some(t) = name_tok(c, n) else { continue };
        let name = c.text_of(t);
        if !name.is_empty() && !name.ends_with("_if") {
            s.add(
                "SV-NAM-003",
                "InterfaceNamingConvention",
                Severity::Warning,
                format!(
                    "Interface '{name}' does not end with required suffix '_if'. \
                     SystemVerilog methodology standards require interface identifiers to end with '_if' (e.g. '{name}_if') to distinguish interfaces from modules and testbench classes."
                ),
                c.loc(t),
            );
        }
    }
}

fn task_return_type(c: &Cst, s: &mut Sink) {
    for n in c.nodes(TaskDecl) {
        let Some(rt) = c.tree.child(n, ReturnType) else { continue };
        let ret = c.trimmed(rt);
        if ret.is_empty() || ret == "void" {
            continue;
        }
        let Some(first) = c.first_tok(rt) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.trimmed(x)).unwrap_or_else(|| "task".into());
        s.add(
            "SV-TYP-010",
            "TaskWithNonVoidReturnType",
            Severity::Error,
            format!(
                "Task '{name}' is declared with non-void return type '{ret}'. In SystemVerilog (IEEE 1800 § 13.3), tasks cannot return values \
                 and cannot specify a return type. Use a 'function' if a return value is required, or remove the return type."
            ),
            c.loc(first),
        );
    }
}

fn unlabeled_assertion(c: &Cst, s: &mut Sink) {
    let mut found: Vec<(NodeId, bool)> = c.nodes(AssertStmt).into_iter().map(|n| (n, false)).collect();
    found.extend(c.nodes(ConcurrentAssert).into_iter().map(|n| (n, true)));
    for (n, concurrent) in found {
        if c.tree.child(n, StmtLabel).is_some() {
            continue;
        }
        let Some(kw) = c.keyword(n) else { continue };
        let kw_text = c.text_of(kw);
        let msg = if concurrent {
            let full = match c.toks(n).nth(1).map(|t| c.text_of(t)) {
                Some(p @ ("property" | "sequence")) => format!("{kw_text} {p}"),
                _ => kw_text.to_string(),
            };
            format!(
                "Concurrent assertion '{full}' lacks an explicit label prefix (e.g. 'check_handshake: {full} (...)'). \
                 Assertions without labels cannot be targeted by simulator severity/assertion control commands (+assert_filter, $assertoff)."
            )
        } else {
            format!(
                "Immediate assertion '{kw_text}' lacks an explicit label prefix (e.g. 'check_val: {kw_text} (...)'). \
                 Unlabeled assertions impede automated regression failure triage, ticket assignment, and coverage correlation."
            )
        };
        s.add("SVA-LBL-001", "UnlabeledAssertion", Severity::Warning, msg, c.loc(kw));
    }
}

/// slang's `ArrayOrRandomizeMethodExpression` with a constraint block:
/// `x.randomize() with { ... }` is a call whose `WithClause` holds one.
fn empty_randomize_with(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(w) = c.tree.child(n, WithClause) else { continue };
        let Some(block) = c.tree.child(w, ConstraintBlock) else { continue };
        let Some(method) = c.tree.children(n).next() else { continue };
        if method == w || !c.text(method).contains("randomize") {
            continue;
        }
        if c.tree.children(block).next().is_some() {
            continue;
        }
        let Some(with) = c.tok(w, "with") else { continue };
        s.add(
            "SV-RND-014",
            "EmptyRandomizeWith",
            Severity::Warning,
            "Inline constraint block 'with { }' is empty. \
             Empty inline constraints construct unnecessary dynamic solver scopes at runtime with zero constraints added. \
             Remove the redundant 'with { }' block."
                .into(),
            c.loc(with),
        );
    }
}

fn static_constraint(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConstraintDecl) {
        let Some(st) = c.toks(n).take_while(|&t| c.text_of(t) != "constraint").find(|&t| c.text_of(t) == "static")
        else {
            continue;
        };
        let name = c.tree.child(n, Name).map(|x| c.trimmed(x)).unwrap_or_else(|| "unnamed".into());
        s.add(
            "SV-RND-009",
            "StaticConstraint",
            Severity::Warning,
            format!(
                "Constraint '{name}' is declared 'static'. In SystemVerilog (IEEE 1800 § 18.5.2), static constraints are shared across all instances of the class; \
                 dynamically disabling or altering them creates cross-instance state pollution and non-deterministic test failures."
            ),
            c.loc(st),
        );
    }
}

/// A cross's label and the coverpoints it names (slang's `items`).
fn cross_parts(c: &Cst, n: NodeId) -> (Option<String>, Vec<NodeId>) {
    let label = c.tree.child(n, StmtLabel).and_then(|l| c.keyword(l)).map(|t| c.text_of(t).to_string());
    let mut items = Vec::new();
    for k in c.kids(n) {
        match *k {
            Child::Token(t) if matches!(c.text_of(t), "iff" | "{" | ";") => break,
            Child::Node(x) if c.kind(x) == NameExpr => items.push(x),
            _ => {}
        }
    }
    (label, items)
}

fn cross_same_coverpoint(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (label, items) = cross_parts(c, n);
        let mut seen = std::collections::HashSet::new();
        for item in items {
            let name = c.trimmed(item);
            if name.is_empty() || seen.insert(name.clone()) {
                continue;
            }
            let Some(first) = c.first_tok(item) else { continue };
            let cross = label.clone().unwrap_or_else(|| "unnamed".into());
            s.add(
                "SV-COV-003",
                "CrossSameCoverpoint",
                Severity::Error,
                format!(
                    "Coverage cross '{cross}' crosses coverpoint '{name}' with itself. Crossing identical coverpoints creates redundant diagonal bins and skews regression coverage metrics."
                ),
                c.loc(first),
            );
        }
    }
}

fn cross_with_empty_bins(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (label, items) = cross_parts(c, n);
        if items.len() >= 2 {
            continue;
        }
        let Some(kw) = c.tok(n, "cross") else { continue };
        let label = label.unwrap_or_else(|| "unnamed_cross".into());
        s.add(
            "SV-COV-008",
            "CrossWithEmptyBins",
            Severity::Warning,
            format!(
                "Covergroup cross '{label}' specifies fewer than 2 coverpoints. Cross coverage requires at least two distinct coverpoints to construct a Cartesian product."
            ),
            c.loc(kw),
        );
    }
}

fn empty_covergroup(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CovergroupDecl) {
        if c.tree.children(n).any(|x| matches!(c.kind(x), Coverpoint | Cross)) {
            continue;
        }
        let name = name_tok(c, n);
        let cg = name.map(|t| c.text_of(t)).filter(|t| !t.is_empty()).unwrap_or("unnamed_covergroup");
        let Some(at) = name.or_else(|| c.first_tok(n)) else { continue };
        s.add(
            "SV-COV-007",
            "EmptyCovergroup",
            Severity::Warning,
            format!(
                "Covergroup '{cg}' is declared with zero coverpoints or crosses. \
                 Empty covergroups collect zero verification metrics and provide a false impression of coverage closure. \
                 Define coverpoints or crosses inside the covergroup, or remove the unused declaration."
            ),
            c.loc(at),
        );
    }
}

fn default_bin(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Bins) {
        if c.tok(n, "bins").is_none() {
            continue;
        }
        let Some(eq) = c.tok(n, "=") else { continue };
        if !c.toks(n).skip_while(|&t| t != eq).nth(1).is_some_and(|t| c.text_of(t) == "default") {
            continue;
        }
        let Some(t) = name_tok(c, n) else { continue };
        s.add(
            "SV-COV-009",
            "CoverpointDefaultBinHazard",
            Severity::Warning,
            format!(
                "Coverpoint bin '{}' is defined with 'default' initializer. Catch-all 'default' bins group all uncharacterized stimulus values into a single bin, artificially inflating coverage closure metrics and masking unhandled corner cases. Use explicit bin ranges, 'ignore_bins', or 'illegal_bins' instead.",
                c.text_of(t)
            ),
            c.loc(t),
        );
    }
}

fn weight_zero(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CoverageOption) {
        let Some(e) = c.items(n).into_iter().next() else { continue };
        let text: String = c.text(e).chars().filter(|&ch| ch != '\n' && ch != '\r').collect();
        if !text.contains("weight") || c.kind(e) != BinaryExpr {
            continue;
        }
        if !c.toks(e).next().is_some_and(|t| c.text_of(t) == "=") {
            continue;
        }
        let Some(right) = c.tree.children(e).nth(1) else { continue };
        let r: String = c.text(right).chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
        if !matches!(r.as_str(), "0" | "1'b0" | "'0" | "32'd0") {
            continue;
        }
        let target = if text.contains("type_option") { "type_option.weight" } else { "option.weight" };
        let Some(first) = c.first_tok(e) else { continue };
        s.add(
            "SV-COV-014",
            "CoverpointWeightZero",
            Severity::Warning,
            format!(
                "Coverage option 'weight' is explicitly configured with value 0 ('{target} = {r}'). \
                 In IEEE 1800 § 19.5 coverage methodology, weight = 0 silently \
                 excludes this covergroup or coverpoint from cumulative functional coverage metric computations, \
                 creating a misleading impression of active coverage tracking without contributing to tape-out closure."
            ),
            c.loc(first),
        );
    }
}

fn missing_iff(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CovergroupDecl) {
        let Some(ev) = c.tree.children(n).find(|&x| matches!(c.kind(x), TimingControl | BlockEvent | WithClause))
        else {
            continue;
        };
        let text = c.text(ev);
        if !text.contains('@') || text.contains("iff") {
            continue;
        }
        let Some(first) = c.first_tok(ev) else { continue };
        let cg = name_tok(c, n).map(|t| c.text_of(t)).unwrap_or("");
        s.add(
            "SV-COV-015",
            "MissingIffOnCovergroupClock",
            Severity::Warning,
            format!(
                "Covergroup '{cg}' is sampled by clocking event '{text}' without an 'iff (!reset)' guard condition. In functional coverage methodology \
                 (IEEE 1800 § 19.4), clock-sampled covergroups should include an 'iff' \
                 guard to disable sampling during active reset, preventing reset-state transactions from contaminating closure metrics."
            ),
            c.loc(first),
        );
    }
}

/// slang's IntegerVectorExpression (`4'b1`, `'hF`) or IntegerLiteralExpression (`7`).
fn integer_literal(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) != Literal {
        return false;
    }
    let toks: Vec<&str> = c.toks(n).map(|t| c.text_of(t)).collect();
    let based = |t: &str| {
        let b = t.strip_prefix('\'').map(|r| r.trim_start_matches(['s', 'S']));
        b.is_some_and(|r| r.starts_with(['b', 'B', 'o', 'O', 'd', 'D', 'h', 'H']))
    };
    match toks.as_slice() {
        [t] => t.chars().next().is_some_and(|ch| ch.is_ascii_digit()) && t.chars().all(|ch| ch.is_ascii_digit() || ch == '_') || based(t),
        _ => toks.iter().any(|t| based(t)),
    }
}

fn constant_wait(c: &Cst, s: &mut Sink) {
    for n in c.nodes(WaitStmt) {
        if !c.toks(n).nth(1).is_some_and(|t| c.text_of(t) == "(") {
            continue;
        }
        let Some(e) = c.items(n).into_iter().next() else { continue };
        let expr = c.trimmed(e);
        let constant = matches!(
            expr.as_str(),
            "0" | "1" | "1'b0" | "1'b1" | "1'd0" | "1'd1" | "32'd0" | "32'd1" | "true" | "false"
        ) || integer_literal(c, e);
        if !constant {
            continue;
        }
        let Some(first) = c.first_tok(e) else { continue };
        s.add(
            "SV-CON-004",
            "ConstantWaitCondition",
            Severity::Error,
            format!(
                "Procedural 'wait({expr})' specifies a compile-time constant condition. \
                 'wait(0)' deadlocks the execution thread permanently; 'wait(1)' evaluates instantaneously without synchronization. \
                 Use a dynamic signal expression or explicit event control ('@(event)')."
            ),
            c.loc(first),
        );
    }
}

fn disable_fork(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DisableStmt) {
        if c.tok(n, "fork").is_none() {
            continue;
        }
        let mut enclosed = false;
        let mut cur = c.parent(n);
        while let Some(p) = cur {
            match c.kind(p) {
                FunctionDecl | TaskDecl | ModuleDecl | ClassDecl => break,
                ParBlock => {
                    enclosed = true;
                    break;
                }
                _ => cur = c.parent(p),
            }
        }
        if enclosed {
            continue;
        }
        let Some(kw) = c.tok(n, "disable") else { continue };
        s.add(
            "SV-CON-010",
            "DisableForkWithoutScope",
            Severity::Error,
            "Unenclosed 'disable fork;' executed directly at routine scope terminates all active child processes of the calling thread. \
             In IEEE 1800 § 9.6.3 concurrency guidelines, 'disable fork;' must be enclosed inside an isolating \
             'fork begin ... disable fork; end join' block to avoid inadvertently killing sibling or parent threads."
                .into(),
            c.loc(kw),
        );
    }
}

fn phase_jump(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(left) = c.tree.children(n).next() else { continue };
        if c.last_text(left) != "jump" {
            continue;
        }
        let text = c.text(left);
        if !text.contains("phase") && !text.contains("domain") {
            continue;
        }
        let Some(first) = c.first_tok(left) else { continue };
        s.add(
            "UVM-PHS-007",
            "UvmPhaseJumpHazard",
            Severity::Warning,
            "Manual UVM phase jump via 'phase.jump()' abruptly interrupts the UVM phasing schedule. \
             Dynamic phase jumping causes phase objection leaks, scoreboard desynchronization, and dangling TLM transactions. \
             Coordinate testbench phases using standard sequences, events, or reset callbacks instead."
                .into(),
            c.loc(first),
        );
    }
}

const DIRECTIONS: [&str; 4] = ["input", "output", "inout", "ref"];

fn legacy_reg_wire(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DataDecl) {
        // A direction makes it slang's PortDeclaration, and a struct or
        // union member is a StructUnionMember.
        if c.toks(n).any(|t| DIRECTIONS.contains(&c.text_of(t))) || c.parent(n).is_some_and(|p| c.kind(p) == DataType) {
            continue;
        }
        if let Some(kw) = c.keyword(n) {
            // A net keyword first: slang's NetDeclaration.
            if c.text_of(kw) == "wire" && c.kids(n).first() == Some(&Child::Token(kw)) {
                s.add(
                    "SV-BAN-001",
                    "LegacyRegWireInTestbench",
                    Severity::Info,
                    "Keyword 'wire' is discouraged in SystemVerilog verification code. \
                     In testbenches and interfaces, use 'logic' or 'virtual interface' instead."
                        .into(),
                    c.loc(kw),
                );
                continue;
            }
        }
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        if c.kids(n).first() != Some(&Child::Node(ty)) {
            continue;
        }
        let Some(first) = c.keyword(ty) else { continue };
        if c.text_of(first) == "reg" {
            s.add(
                "SV-BAN-001",
                "LegacyRegWireInTestbench",
                Severity::Info,
                "Legacy Verilog-95 keyword 'reg' is discouraged in modern SystemVerilog testbenches. \
                 Use 'logic' for unified 4-state storage across procedural and continuous assignments."
                    .into(),
                c.loc(first),
            );
        }
    }
}

fn non_ansi_ports(c: &Cst, s: &mut Sink) {
    // Module, interface and program headers: `module m(a, b);`.
    for h in c.nodes(Header) {
        let Some(list) = c.tree.child(h, PortList) else { continue };
        let Some(port) = c.tree.children(list).next() else { continue };
        let plain = c.kind(port) == Port
            && c.kids(port).iter().all(|k| matches!(*k, Child::Node(x) if c.kind(x) == Declarator))
            && c.tree.children(port).next().is_some_and(|d| {
                c.kids(d).len() == 1 && c.tree.child(d, Name).is_some()
            });
        let explicit = c.kind(port) == Port && c.keyword(port).is_some_and(|t| matches!(c.text_of(t), "." | "{"));
        if !(plain || explicit) {
            continue;
        }
        let Some(open) = c.tok(list, "(") else { continue };
        s.add(
            "SV-DEP-006",
            "NonAnsiPortList",
            Severity::Info,
            "Non-ANSI port list declaration is obsolete in IEEE 1800 § 13.3. \
             Separate declaration of port names in header and types in body introduces redundant typing and syntax hazards. \
             Use modern ANSI-style inline port declarations: 'module name(input logic [7:0] a, output logic b)'."
                .into(),
            c.loc(open),
        );
    }
    // Tasks and functions with port declarations in the body.
    let mut routines = c.nodes(FunctionDecl);
    routines.extend(c.nodes(TaskDecl));
    for n in routines {
        if c.tree.child(n, PortList).is_some() {
            continue;
        }
        let Some(body) = c.tree.child(n, Body) else { continue };
        let Some(item) = c
            .tree
            .children(body)
            .find(|&x| c.kind(x) == DataDecl && c.keyword(x).is_some_and(|t| DIRECTIONS.contains(&c.text_of(t))))
        else {
            continue;
        };
        let Some(first) = c.first_tok(item) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.text(x)).unwrap_or_default();
        s.add(
            "SV-DEP-006",
            "NonAnsiPortList",
            Severity::Info,
            format!(
                "Routine '{name}' uses obsolete non-ANSI port declarations inside the routine body. \
                 Declare argument directions and types directly in the routine header (e.g. 'task {name}(input bit [7:0] data, output bit err)')."
            ),
            c.loc(first),
        );
    }
}
