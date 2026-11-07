//! UVMCheck's rules in Rust, over [`dv_model::Design`] and `sv-syntax` trees.
//!
//! Each rule is a line-for-line port of its C++ twin under `src/rules/`.
//! Model rules read only the class model, so any front end that fills a
//! `Design` (slang or the native sv-sema) can feed them; syntax rules (see
//! [`syntax`]) read the native syntax tree. Messages are copied verbatim so
//! findings compare one to one with `uvmcheck` (see `tools/lint_parity.py`).

use dv_model::{Class, ClassId, Design, Direction, Loc, Method, PhaseKind, Property, TypeShape};

mod factory;
mod g_uvm;
mod g_uvm2;
mod g_misc;
mod g_design;
mod g_design_class;
mod g_design_metrics;
mod g_design_spec;
mod g_design_types;
mod g_bug;
mod g_lang;
mod g_port;
mod g_port2;
mod g_port3;
pub mod g_exprty;
mod g_types;
mod g_history;
pub use g_history::set_history;
mod uvm_api;
mod waivers;
mod g_c_support;
mod g_concurrency;
mod g_sva_cov;
mod g_deprecated;
mod g_readability;
mod g_readability2;
mod g_style;
mod g_style2;
mod g_random;
mod g_random_bug;
mod g_random_dist;
mod g_random_oop;
mod g_random_sema;
mod g_random_types;
mod g_meth_body;
mod g_meth_body2;
mod g_meth_config;
mod g_meth_dup;
mod g_meth_gotcha;
mod g_meth_model;
mod g_meth_pattern;
mod g_meth_string;
mod g_meth_syntax;
mod g_methodology;
pub mod syntax;
mod syntax2;
mod syntax3;
mod syntax4;
mod syntax5;
mod syntax6;
mod uvm;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Style,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Style => "STYLE",
            Severity::Warning => "WARNING",
            Severity::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub rule: &'static str,
    pub name: &'static str,
    pub severity: Severity,
    pub message: String,
    pub loc: Loc,
    /// Secondary locations ("non-virtual method in 'base'").
    pub related: Vec<(Loc, String)>,
}

impl Finding {
    /// File, line and column as uvmcheck reports them: for code from a macro,
    /// the line of the outermost expansion with column 0.
    pub fn position<'d>(&self, d: &'d Design) -> (&'d str, u32, u32) {
        self.position_in(d.files())
    }

    /// [`Self::position`] with a file table (index 0 "no file").
    pub fn position_in<'f>(&self, files: &'f [String]) -> (&'f str, u32, u32) {
        let name = |f: u32| files.get(f as usize).map(String::as_str).unwrap_or("");
        match self.loc.expansion {
            Some((file, line)) => (name(file), line, 0),
            None => (name(self.loc.file), self.loc.line, self.loc.column),
        }
    }
}

/// Collects findings as rules emit them.
#[derive(Default)]
pub struct Sink {
    pub findings: Vec<Finding>,
}

impl Sink {
    fn add(
        &mut self,
        rule: &'static str,
        name: &'static str,
        severity: Severity,
        message: String,
        loc: Loc,
    ) -> &mut Finding {
        self.findings.push(Finding {
            rule,
            name,
            severity,
            message,
            loc,
            related: Vec::new(),
        });
        self.findings.last_mut().unwrap()
    }
}

pub type RuleFn = fn(&Design, &mut Sink);

/// A rule that reads the class model and the syntax trees together (the
/// C++ rules that walk slang's AST of method bodies): it gets every tree at
/// once, numbered as the design numbers files.
pub type MixedFn = fn(&Design, &[syntax::Cst], &mut Sink);

/// What a rule reads.
#[derive(Clone, Copy)]
pub enum Check {
    Model(RuleFn),
    Syntax(syntax::SyntaxFn),
    Mixed(MixedFn),
}

/// A ported rule: its id, name and check.
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub check: Check,
}

impl Rule {
    pub fn is_syntax(&self) -> bool {
        matches!(self.check, Check::Syntax(_) | Check::Mixed(_))
    }
}

/// Every ported rule, in the order uvmcheck registers them (by id).
static TOPS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Adds top modules (`-top`): elaboration starts from them only.
pub fn add_tops(tops: &[String]) {
    if let Ok(mut t) = TOPS.write() {
        t.extend(tops.iter().cloned());
    }
}

/// The tops `-top` named (empty: every module nothing instantiates).
pub(crate) fn tops() -> Vec<String> {
    TOPS.read().map(|t| t.clone()).unwrap_or_default()
}

/// Ids a rule reports under besides its own (a rule engine's sub-rules).
pub fn sub_rules(id: &str) -> &'static [(&'static str, &'static str)] {
    match id {
        "SV-STY-100" => g_port3::STYLE_SUBS,
        "SV-NAM-100" => g_port3::NAMING_SUBS,
        _ => &[],
    }
}

pub fn rules() -> Vec<Rule> {
    let mut v = vec![
        Rule {
            id: "SV-OOP-003",
            name: "MissingSuperNew",
            check: crate::Check::Model(missing_super_new),
        },
        Rule {
            id: "SV-OOP-001",
            name: "NonVirtualMethodOverride",
            check: crate::Check::Model(non_virtual_method_override),
        },
        Rule {
            id: "SV-OOP-007",
            name: "StaticMemberInUvmComponent",
            check: crate::Check::Model(static_member_in_uvm_component),
        },
        Rule {
            id: "SV-OOP-014",
            name: "SuperNewNotFirstStatement",
            check: crate::Check::Model(super_new_not_first_statement),
        },
        Rule {
            id: "SV-OOP-004",
            name: "VariableShadowing",
            check: crate::Check::Model(variable_shadowing),
        },
        Rule {
            id: "SV-OOP-005",
            name: "VirtualCallInConstructor",
            check: crate::Check::Model(virtual_call_in_constructor),
        },
        Rule {
            id: "SV-OOP-015",
            name: "VirtualMethodSignatureMismatch",
            check: crate::Check::Model(virtual_method_signature_mismatch),
        },
        Rule {
            id: "SV-PRF-001",
            name: "PassArrayByValue",
            check: crate::Check::Model(pass_array_by_value),
        },
        Rule {
            id: "SV-RND-002",
            name: "GlobalRngInClass",
            check: crate::Check::Model(global_rng_in_class),
        },
        Rule {
            id: "SV-RND-018",
            name: "RandomizeInConstructor",
            check: crate::Check::Model(randomize_in_constructor),
        },
        Rule {
            id: "SV-RND-003",
            name: "RandomizeMissingSuper",
            check: crate::Check::Model(randomize_missing_super),
        },
        Rule {
            id: "SV-STY-001",
            name: "ClassMemberPrefix",
            check: crate::Check::Model(class_member_prefix),
        },
        Rule {
            id: "UVM-STY-002",
            name: "ConstraintNameSuffix",
            check: crate::Check::Model(constraint_name_suffix),
        },
        Rule {
            id: "SV-NAM-001",
            name: "UvmComponentNamingConvention",
            check: crate::Check::Model(uvm_component_naming_convention),
        },
    ];
    v.extend(uvm::rules());
    v.extend(factory::rules());
    v.extend(syntax::rules());
    v.extend(syntax2::rules());
    v.extend(syntax3::rules());
    v.extend(syntax4::rules());
    v.extend(syntax5::rules());
    v.extend(syntax6::rules());
    v.extend(g_uvm::rules());
    v.extend(g_uvm2::rules());
    v.extend(g_misc::rules());
    v.extend(g_design::rules());
    v.extend(g_concurrency::rules());
    v.extend(g_sva_cov::rules());
    v.extend(g_style::rules());
    v.extend(g_deprecated::rules());
    v.extend(g_style2::rules());
    v.extend(g_readability::rules());
    v.extend(g_readability2::rules());
    v.extend(g_random::rules());
    v.extend(g_random_oop::rules());
    v.extend(g_random_bug::rules());
    v.extend(g_random_dist::rules());
    v.extend(g_random_types::rules());
    v.extend(g_methodology::rules());
    v.extend(g_bug::rules());
    v.extend(g_lang::rules());
    v.extend(g_port::rules());
    v.extend(g_port2::rules());
    v.extend(g_port3::rules());
    v.extend(g_types::rules());
    v.extend(g_history::rules());
    v.sort_by_key(|r| r.id);
    v
}

/// Runs the rules whose ids pass `keep` and returns their findings in a stable
/// order (file, line, column, rule). As in uvmcheck's DiagnosticEngine, nothing
/// is reported in the test UVM mock (`uvm_mock.svh`).
/// Only model rules run here; see [`lint_all`] for syntax rules too.
pub fn lint(design: &Design, keep: impl Fn(&str) -> bool) -> Vec<Finding> {
    lint_all(design, &[], keep).1
}

/// Runs model rules on `design` and syntax rules on `trees` (the trees the
/// design was built from, or none), returning the file table the findings'
/// locations index (the design's, plus files only syntax rules reached).
pub fn lint_all(
    design: &Design,
    trees: &[sv_syntax::Tree],
    keep: impl Fn(&str) -> bool,
) -> (Vec<String>, Vec<Finding>) {
    let mut sink = Sink::default();
    let all: Vec<Rule> = rules().into_iter().filter(|r| keep(r.id)).collect();
    for r in &all {
        if let Check::Model(run) = r.check {
            run(design, &mut sink);
        }
    }
    let mut locator = sv_sema::Locator::new(design.files().to_vec());
    let mixed = all.iter().any(|r| matches!(r.check, Check::Mixed(_)));
    if all.iter().any(Rule::is_syntax) {
        let mut prior: std::collections::HashSet<std::path::PathBuf> = Default::default();
        let spec: std::rc::Rc<std::cell::OnceCell<_>> = Default::default();
        for (ix, tree) in trees.iter().enumerate() {
            let mut cst = syntax::Cst::new(tree, ix, locator);
            cst.prior_files = std::rc::Rc::new(prior.clone());
            cst.all = trees;
            cst.spec = spec.clone();
            prior.extend(tree.files.iter().cloned());
            for r in &all {
                if let Check::Syntax(run) = r.check {
                    run(&cst, &mut sink);
                }
            }
            if mixed {
                // Number every file of the tree now, so that the trees of
                // the mixed pass below share one file table.
                for f in 0..tree.files.len() as u32 {
                    cst.loc_at(sv_syntax::Origin { file: f, offset: 0 });
                }
            }
            locator = cst.into_locator();
        }
    }
    let files = locator.into_files();
    if mixed {
        let csts: Vec<syntax::Cst> = trees
            .iter()
            .enumerate()
            .map(|(ix, t)| {
                let mut c = syntax::Cst::new(t, ix, sv_sema::Locator::new(files.clone()));
                c.all = trees;
                c
            })
            .collect();
        for r in &all {
            if let Check::Mixed(run) = r.check {
                run(design, &csts, &mut sink);
            }
        }
    }
    let mut f = sink.findings;
    f.retain(|x| !x.position_in(&files).0.contains("uvm_mock.svh"));
    waivers::apply(&mut f, &files);
    f.sort_by(|a, b| (a.position_in(&files), a.rule).cmp(&(b.position_in(&files), b.rule)));
    (files, f)
}

/// Visits of each generic class's body, one line per class, by the syntax
/// engine and by the model: `<syntax count> <name>@<file>:<line> <model count>`.
pub fn spec_report(design: &Design, trees: &[sv_syntax::Tree]) -> Vec<String> {
    let mut locator = sv_sema::Locator::new(design.files().to_vec());
    let mut rows = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let cst = syntax::Cst::new(t, ix, locator);
        for (node, k) in g_design_spec::spec_counts(&cst) {
            let Some(nm) = cst.tree.child(sv_syntax::NodeId(node), sv_syntax::SyntaxKind::Name) else {
                continue;
            };
            let Some(tok) = cst.first_tok(nm) else {
                continue;
            };
            rows.push((k, cst.tree.bare_text(nm), cst.loc(tok), ix, sv_syntax::NodeId(node)));
        }
        locator = cst.into_locator();
    }
    let files = locator.into_files();
    let base = |f: u32| {
        files
            .get(f as usize)
            .map(|p| p.rsplit('/').next().unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let _ = design;
    let model = sv_sema::spec_counts(trees);
    let mut out: Vec<String> = rows
        .into_iter()
        .map(|(k, name, l, ix, node)| {
            let m = model.get(&(ix, node)).map(|m| m.to_string()).unwrap_or("-".into());
            // A class from a macro is where the macro is used, as slang says.
            let (f, line) = l.expansion.unwrap_or((l.file, l.line));
            format!("{k} {name}@{}:{line} {m}", base(f))
        })
        .collect();
    out.sort();
    out
}

// ---- model helpers with the C++ facade's exact semantics ----

pub(crate) fn find_method<'a>(c: &'a Class, name: &str) -> Option<&'a Method> {
    c.methods.iter().find(|m| m.name == name)
}

pub(crate) fn find_property<'a>(c: &'a Class, name: &str) -> Option<&'a Property> {
    c.properties.iter().find(|p| p.name == name)
}

pub(crate) fn ancestors(d: &Design, c: &Class) -> Vec<ClassId> {
    d.ancestors(c.id).map(|a| a.id).collect()
}

/// `Model::derivesFrom`: the class or an ancestor is named `name`, or the last
/// resolved class names `name` as its (unresolved) base.
pub(crate) fn derives_from(d: &Design, id: Option<ClassId>, name: &str) -> bool {
    let mut id = id;
    let mut steps = 0;
    while let Some(i) = id {
        if steps > d.classes().len() {
            break;
        }
        steps += 1;
        let c = d.class(i);
        if c.name == name {
            return true;
        }
        if c.base.is_none() {
            return c.base_name == name;
        }
        id = c.base;
    }
    false
}

pub(crate) const FUNCTION_PHASES: &[&str] = &[
    "build_phase",
    "connect_phase",
    "end_of_elaboration_phase",
    "start_of_simulation_phase",
    "extract_phase",
    "check_phase",
    "report_phase",
    "final_phase",
];

pub(crate) const TASK_PHASES: &[&str] = &[
    "run_phase",
    "pre_reset_phase",
    "reset_phase",
    "post_reset_phase",
    "pre_configure_phase",
    "configure_phase",
    "post_configure_phase",
    "pre_main_phase",
    "main_phase",
    "post_main_phase",
    "pre_shutdown_phase",
    "shutdown_phase",
    "post_shutdown_phase",
];

pub(crate) fn is_function_phase(n: &str) -> bool {
    FUNCTION_PHASES.contains(&n)
}

pub(crate) fn is_task_phase(n: &str) -> bool {
    TASK_PHASES.contains(&n)
}

#[allow(dead_code)]
pub(crate) fn phase_kind(n: &str) -> PhaseKind {
    if is_function_phase(n) {
        PhaseKind::Function
    } else if is_task_phase(n) {
        PhaseKind::Task
    } else {
        PhaseKind::None
    }
}

// ---- OOP ----

fn missing_super_new(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let Some(base) = cls.base else { continue };
        let Some(ctor) = find_method(cls, "new") else {
            continue;
        };
        if ctor.is_builtin || ctor.calls_super_new {
            continue;
        }
        out.add(
            "SV-OOP-003",
            "MissingSuperNew",
            Severity::Warning,
            format!(
                "Constructor 'new' in class '{}' derives from '{}' but does not explicitly call super.new(...).",
                cls.name,
                d.class(base).name
            ),
            ctor.loc,
        );
    }
}

fn non_virtual_method_override(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        let chain = ancestors(d, cls);
        for sub in &cls.methods {
            if sub.is_extern || sub.name == "new" || sub.is_builtin {
                continue;
            }
            for &a in &chain {
                let anc = d.class(a);
                let Some(bm) = find_method(anc, &sub.name) else {
                    continue;
                };
                if bm.is_builtin {
                    break;
                }
                if !bm.is_virtual {
                    let f = out.add(
                        "SV-OOP-001",
                        "NonVirtualMethodOverride",
                        Severity::Warning,
                        format!("Method '{}' in class '{}' overrides non-virtual method in base class '{}'.", sub.name, cls.name, anc.name),
                        sub.loc,
                    );
                    f.related
                        .push((bm.loc, format!("non-virtual method in '{}'", anc.name)));
                }
                break;
            }
        }
    }
}

fn static_member_in_uvm_component(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for p in &cls.properties {
            if !p.is_static || p.is_const {
                continue;
            }
            out.add(
                "SV-OOP-007",
                "StaticMemberInUvmComponent",
                Severity::Error,
                format!(
                    "Non-const static member property '{}' in uvm_component class '{}'. In SystemVerilog, static class members are shared across all instances \
simulation-wide. In multi-channel environments (e.g. multiple agents), instances will overwrite each other's state \
causing race conditions. Use an instance-scoped member or a dedicated singleton service instead.",
                    p.name, cls.name
                ),
                p.loc,
            );
        }
    }
}

fn super_new_not_first_statement(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        let Some(ctor) = find_method(cls, "new") else {
            continue;
        };
        if ctor.is_builtin {
            continue;
        }
        let Some((idx, loc)) = ctor.super_new else {
            continue;
        };
        if idx == 0 {
            continue;
        }
        out.add(
            "SV-OOP-014",
            "SuperNewNotFirstStatement",
            Severity::Warning,
            format!(
                "Constructor 'new' in derived class '{}' invokes 'super.new(...)' after other procedural statements. In SystemVerilog OOP \
(IEEE 1800 § 8.17), 'super.new(...)' must be the \
very first procedural statement in a derived class constructor to ensure base class properties \
and infrastructure are fully initialized before derived logic executes.",
                cls.name
            ),
            loc,
        );
    }
}

fn variable_shadowing(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let chain = ancestors(d, cls);
        for p in &cls.properties {
            for &b in &chain {
                let base = d.class(b);
                let Some(shadow) = find_property(base, &p.name) else {
                    continue;
                };
                let f = out.add(
                    "SV-OOP-004",
                    "VariableShadowing",
                    Severity::Warning,
                    format!(
                        "Class property '{}' in derived class '{}' shadows property of the same name in base class '{}'. Accessing through a base class handle will reference the base member.",
                        p.name, cls.name, base.name
                    ),
                    p.loc,
                );
                f.related
                    .push((shadow.loc, format!("shadowed property in '{}'", base.name)));
                break;
            }
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            if sub.name != "new" {
                for a in &sub.arguments {
                    let Some(cp) = find_property(cls, &a.name) else {
                        continue;
                    };
                    let f = out.add(
                        "SV-OOP-004",
                        "VariableShadowing",
                        Severity::Warning,
                        format!(
                            "Method argument '{}' in method '{}' shadows class property '{}'. Use 'this.{}' or rename the argument to prevent silent assignment bugs.",
                            a.name, sub.name, cp.name, cp.name
                        ),
                        a.loc,
                    );
                    f.related.push((cp.loc, "class property".into()));
                }
            }
            for v in &sub.locals {
                let Some(cp) = find_property(cls, &v.name) else {
                    continue;
                };
                out.add(
                    "SV-OOP-004",
                    "VariableShadowing",
                    Severity::Warning,
                    format!(
                        "Local variable '{}' in method '{}' shadows class property '{}'. Rename the local variable to avoid confusion.",
                        v.name, sub.name, cp.name
                    ),
                    v.loc,
                );
            }
        }
    }
}

fn virtual_call_in_constructor(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "new" || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.is_system || !c.callee_declared_virtual {
                    continue;
                }
                out.add(
                    "SV-OOP-005",
                    "VirtualCallInConstructor",
                    Severity::Warning,
                    format!(
                        "Virtual method '{}' is invoked inside constructor 'new' of class '{}'. In SystemVerilog, base class constructors execute before derived class members are initialized. \
Calling virtual methods in constructors executes overridden child methods against uninitialized state.",
                        c.name, cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}

fn virtual_method_signature_mismatch(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let Some(base) = cls.base else { continue };
        let base = d.class(base);
        for sub in &cls.methods {
            if sub.is_extern || sub.name == "new" {
                continue;
            }
            let Some(bs) = find_method(base, &sub.name) else {
                continue;
            };
            if !bs.declared_virtual {
                continue;
            }
            let (da, ba) = (sub.arguments.len(), bs.arguments.len());
            if da != ba {
                out.add(
                    "SV-OOP-015",
                    "VirtualMethodSignatureMismatch",
                    Severity::Warning,
                    format!(
                        "Method '{}' in derived class '{}' defines {} parameter(s), but overrides virtual method in base class '{}' defined with {} parameter(s). In SystemVerilog OOP \
(IEEE 1800 § 8.20), virtual method overrides must match base signature parameter counts; \
mismatching arguments causes method hiding instead of polymorphic override.",
                        sub.name, cls.name, da, base.name, ba
                    ),
                    sub.loc,
                );
            }
        }
    }
}

// ---- performance, randomization, style ----

fn pass_array_by_value(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for a in &sub.arguments {
                if a.direction != Direction::In || a.shape == TypeShape::Other {
                    continue;
                }
                out.add(
                    "SV-PRF-001",
                    "PassArrayByValue",
                    Severity::Warning,
                    format!(
                        "Argument '{}' of type '{}' in method '{}' is passed by value (input). Use 'const ref' instead.",
                        a.name, a.canonical_type, sub.name
                    ),
                    a.loc,
                );
            }
        }
    }
}

fn global_rng_in_class(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.name != "$urandom" && c.name != "$urandom_range" && c.name != "$random" {
                    continue;
                }
                out.add(
                    "SV-RND-002",
                    "GlobalRngInClass",
                    Severity::Warning,
                    format!(
                        "Invoking global pseudo-random system function '{}' inside class '{}'. Global RNG calls consume the process-level PRNG state instead of the object's hierarchical random stream, \
destroying simulation seed repeatability and testbench reproducibility. Use 'std::randomize()' or class-scoped RNG.",
                        c.name, cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}

fn randomize_in_constructor(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "new" || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.name != "randomize" {
                    continue;
                }
                out.add(
                    "SV-RND-018",
                    "RandomizeInConstructor",
                    Severity::Warning,
                    format!(
                        "Calling 'randomize()' inside class constructor 'new()' of class '{}'. In SystemVerilog OOP, calling randomize() during object construction executes before derived class members, \
constraints, and factory overrides are fully constructed and initialized. \
Stimulus randomization should always occur explicitly on fully constructed instances (e.g. 'if (!item.randomize()) ...').",
                        cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}

fn randomize_missing_super(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            if sub.name != "pre_randomize" && sub.name != "post_randomize" {
                continue;
            }
            let above = d
                .ancestors(cls.id)
                .any(|a| find_method(a, &sub.name).is_some_and(|m| !m.is_builtin));
            if !above {
                continue;
            }
            if sub
                .calls
                .iter()
                .any(|c| !c.is_system && c.name == sub.name && c.via_super)
            {
                continue;
            }
            out.add(
                "SV-RND-003",
                "RandomizeMissingSuper",
                Severity::Warning,
                format!(
                    "Overridden randomization callback '{}' in class '{}' does not invoke 'super.{}()'. \
Unlike UVM phases, SV pre/post_randomize callbacks do not auto-chain up inheritance; \
omitting super bypasses base-class payload, CRC, and sequence updates.",
                    sub.name, cls.name, sub.name
                ),
                sub.loc,
            );
        }
    }
}

fn class_member_prefix(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() || cls.name.starts_with("uvm_") {
            continue;
        }
        for p in &cls.properties {
            if p.is_const || p.is_static || p.name == "type_name" || p.name == "type_id" {
                continue;
            }
            if !p.name.starts_with("m_") {
                out.add(
                    "SV-STY-001",
                    "ClassMemberPrefix",
                    Severity::Info,
                    format!(
                        "Member property '{}' in class '{}' should begin with 'm_' prefix (e.g. 'm_{}') to distinguish internal state from local arguments.",
                        p.name, cls.name, p.name
                    ),
                    p.loc,
                );
            }
        }
    }
}

fn constraint_name_suffix(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for c in &cls.constraints {
            if c.name.ends_with("_c") {
                continue;
            }
            out.add(
                "UVM-STY-002",
                "ConstraintNameSuffix",
                Severity::Warning,
                format!(
                    "Constraint block '{}' in class '{}' does not follow the constraint naming convention. \
Constraint block names must end with '_c' suffix (e.g. '{}_c') to clearly distinguish constraint sets from random variables in solver logs.",
                    c.name, cls.name, c.name
                ),
                c.loc,
            );
        }
    }
}

fn uvm_component_naming_convention(d: &Design, out: &mut Sink) {
    const CHECKS: [(&str, &str, &str); 5] = [
        ("uvm_driver", "_driver", "driver"),
        ("uvm_monitor", "_monitor", "monitor"),
        ("uvm_agent", "_agent", "agent"),
        ("uvm_env", "_env", "environment"),
        ("uvm_test", "_test", "test"),
    ];
    for cls in d.classes() {
        if cls.is_parameterized || cls.name.starts_with("uvm_") || cls.name.is_empty() {
            continue;
        }
        for (base, suffix, role) in CHECKS {
            if derives_from(d, Some(cls.id), base) {
                if !cls.name.ends_with(suffix) {
                    out.add(
                        "SV-NAM-001",
                        "UvmComponentNamingConvention",
                        Severity::Warning,
                        format!(
                            "UVM {} class '{}' derived from '{}' does not end with required suffix '{}'. Following uniform naming conventions is essential for VIP maintainability and automated testbench reflection.",
                            role, cls.name, base, suffix
                        ),
                        cls.loc,
                    );
                }
                break;
            }
        }
    }
}
