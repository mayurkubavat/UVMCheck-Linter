//! UVMCheck's rules in Rust, over [`dv_model::Design`] and `sv-syntax` trees.
//!
//! Each rule is a line-for-line port of its C++ twin under `cpp/src/rules/`.
//! Model rules read only the class model, so any front end that fills a
//! `Design` (slang or the native sv-sema) can feed them; syntax rules (see
//! [`syntax`]) read the native syntax tree. Messages are copied verbatim so
//! findings compare one to one with `uvmcheck` (see `tools/gates/lint_parity.py`).
//!
//! # How a run works
//!
//! [`rules`] lists every rule as a [`Rule`]: an id, a name and a [`Check`]
//! saying what it reads (the model, one tree, or the model and every tree).
//! [`lint_all`] filters them, runs them and returns the findings sorted by
//! position and rule, with inline waivers applied (`waivers`) and nothing
//! reported in the test UVM mock.
//!
//! Rules run on `DV_LINT_JOBS` threads (the machine's cores by default):
//! each thread takes the next rule, slowest first. Work several rules need
//! (expression types, sv-sema's checks, elaboration, specialization counts)
//! is built once per compile in `shared` and waited for by the others; with
//! many rules those caches are warmed first. Findings are put back in the
//! serial order (model, then syntax, then mixed rules, each as registered),
//! so the output does not depend on the thread count. `DV_LINT_PROFILE=1`
//! prints what each rule cost.
//!
//! # Where the rules are
//!
//! One file per rule, under `rules/<family>/`, named after the rule
//! (`rules/uvm/blocking_in_observer.rs` is UVM-OBS-002, BlockingInObserver).
//! The families are the folders of `cpp/src/rules/`, so a rule sits beside
//! its C++ twin's family. Each file opens with the rule's id, name and what
//! it flags, and holds the helpers only that rule uses.
//! `rules/<family>/mod.rs` registers the family's rules and `rules/mod.rs`
//! collects every family.
//!
//! Helpers several rules share live in `support/`, one module per topic
//! (`support::class_index`, `support::statements`, `support::slang_ast` and
//! so on). Beside them: [`syntax::Cst`] (a tree with parents, per-kind node
//! lists and locations), [`expr_types`] (expression types as slang gives
//! them), `shared` (work several rules need, built once per compile),
//! `uvm_api` (the UVM API packs) and `waivers` (inline waivers).

use dv_model::{Class, ClassId, Design, Loc, Method, PhaseKind, Property};

pub mod expr_types;
pub mod frames;
mod rules;
mod support;

pub(crate) mod shared;

pub use crate::support::history::set_history;

mod uvm_api;

mod waivers;

mod rule_profiles;

#[allow(dead_code)]
pub(crate) mod json;
pub mod options;

/// A `naming.<target>.pattern` the native naming engine can check, and
/// whether the target is one it knows.
pub fn naming_pattern_supported(target: &str, pattern: &str) -> bool {
    rules::naming_pattern_supported(target, pattern)
}

/// A `style.file_header_pattern` the native style engine can match.
pub fn header_pattern_supported(p: &str) -> bool {
    options::header_alternatives(p).is_some()
}
pub mod cache;
pub mod changed;
pub mod cli;
pub mod explain;
pub mod fix;
pub mod init;
pub mod plugins;
pub mod project;
pub mod report;
pub mod suppress;
pub mod tbgraph;
pub mod tbmap;

static INLINE_WAIVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many findings inline waiver comments have dropped so far.
pub fn inline_waived() -> usize {
    INLINE_WAIVED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Sets the count [`inline_waived`] reports (a run replayed from the cache).
pub fn set_inline_waived(n: usize) {
    INLINE_WAIVED.store(n, std::sync::atomic::Ordering::Relaxed);
}

/// Drops the findings inline waiver comments waive (findings made outside
/// the rules, such as Python rules'), counting them in [`inline_waived`].
pub fn apply_inline_waivers(found: &mut Vec<Finding>, files: &[String]) {
    INLINE_WAIVED.fetch_add(waivers::apply(found, files), std::sync::atomic::Ordering::Relaxed);
}

/// Which rules run without `--rule`, as uvmcheck selects them: `preset`
/// (`--preset`: soundness, methodology, performance, style, security,
/// smells, advisory, reuse, guide, uvm-1800.2, all; none: every profile but
/// advisory) and `experimental` (`--experimental`). Rules dv-lint has with no
/// uvmcheck twin always run. Err for an unknown preset.
pub fn rule_selection(preset: Option<&str>, experimental: bool) -> Result<impl Fn(&str) -> bool, String> {
    const ALL: u32 = u32::MAX;
    const ADVISORY: u32 = 1 << 6;
    enum Sel {
        Profiles(u32),
        Category(&'static str),
        Guidelines(Vec<&'static str>),
        AnyGuideline,
    }
    let sel = match preset.map(str::to_ascii_lowercase).as_deref() {
        None => Sel::Profiles(ALL & !ADVISORY),
        Some("all") => Sel::Profiles(ALL),
        Some("soundness") => Sel::Profiles(1),
        Some("methodology") => Sel::Profiles(1 << 1),
        Some("performance" | "perf") => Sel::Profiles(1 << 2),
        Some("style") => Sel::Profiles(1 << 3),
        Some("security") => Sel::Profiles(1 << 4),
        Some("smells" | "smell" | "maintainability") => Sel::Profiles(1 << 5),
        Some("advisory") => Sel::Profiles(ADVISORY),
        Some("uvm-1800.2" | "migration") => Sel::Category("Deprecated"),
        Some("reuse") => Sel::Guidelines(vec!["MG-1", "MG-2", "MG-3", "MG-4", "MG-5"]),
        Some("guide") => Sel::AnyGuideline,
        Some(p) => return Err(format!("unknown preset '{p}'")),
    };
    // "MG-4" selects MG-4.1, MG-4.2 ... but not MG-40.1.
    let matches = |s: &str, g: &str| g == s || (g.len() > s.len() && g.starts_with(s) && g.as_bytes()[s.len()] == b'.');
    Ok(move |id: &str| {
        let Some(&(_, category, profiles, guides, exp)) = rule_profiles::RULES.iter().find(|r| r.0 == id) else {
            return true;
        };
        if exp && !experimental {
            return false;
        }
        match &sel {
            Sel::Profiles(m) => profiles & m != 0,
            Sel::Category(c) => category == *c,
            Sel::Guidelines(gs) => guides.iter().any(|g| gs.iter().any(|s| matches(s, g))),
            Sel::AnyGuideline => !guides.is_empty(),
        }
    })
}

pub mod syntax;

/// How serious a finding is, ordered from least to most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational (metrics, suggestions).
    Info,
    /// A style or naming convention.
    Style,
    /// Worth fixing: a likely bug or methodology problem.
    Warning,
    /// Should be fixed.
    Error,
}

impl Severity {
    /// The name uvmcheck prints (`"WARNING"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Style => "STYLE",
            Severity::Warning => "WARNING",
            Severity::Error => "ERROR",
        }
    }
}

/// One finding of one rule.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Rule id (`"SV-OOP-003"`).
    pub rule: &'static str,
    /// Rule name (`"MissingSuperNew"`).
    pub name: &'static str,
    /// Severity, as the C++ rule reports it.
    pub severity: Severity,
    /// The message, word for word as uvmcheck prints it.
    pub message: String,
    /// Where it is (see [`Finding::position`] for where it is reported).
    pub loc: Loc,
    /// Secondary locations ("non-virtual method in 'base'").
    pub related: Vec<(Loc, String)>,
    /// Rewrites that fix it (see [`fix`]).
    pub fixes: Vec<fix::Fix>,
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
    /// Findings in the order rules emitted them.
    pub findings: Vec<Finding>,
}

impl Sink {
    pub(crate) fn add(
        &mut self,
        rule: &'static str,
        name: &'static str,
        severity: Severity,
        message: String,
        loc: Loc,
    ) -> &mut Finding {
        self.findings.push(Finding { rule, name, severity, message, loc, related: Vec::new(), fixes: Vec::new() });
        self.findings.last_mut().unwrap()
    }
}

/// A rule that reads only the class model.
pub type RuleFn = fn(&Design, &mut Sink);

/// A rule that reads the class model and the syntax trees together (the
/// C++ rules that walk slang's AST of method bodies): it gets every tree at
/// once, numbered as the design numbers files.
pub type MixedFn = fn(&Design, &[syntax::Cst], &mut Sink);

/// What a rule reads.
#[derive(Clone, Copy)]
pub enum Check {
    /// Reads only the class model: runs once per compile.
    Model(RuleFn),
    /// Reads one syntax tree: runs once per tree.
    Syntax(syntax::SyntaxFn),
    /// Reads the model and every tree: runs once per compile.
    Mixed(MixedFn),
}

/// A ported rule: its id, name and check.
pub struct Rule {
    /// Rule id (`"SV-OOP-003"`), as uvmcheck registers it.
    pub id: &'static str,
    /// Rule name (`"MissingSuperNew"`).
    pub name: &'static str,
    /// What the rule reads and the function that runs it.
    pub check: Check,
}

impl Rule {
    /// True for rules that need syntax trees (syntax and mixed ones), which the
    /// slang front end does not provide.
    pub fn is_syntax(&self) -> bool {
        matches!(self.check, Check::Syntax(_) | Check::Mixed(_))
    }
}

/// Top modules from `-top` (see [`add_tops`]).
pub(crate) static TOPS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Adds top modules (`-top`): elaboration starts from them only.
pub fn add_tops(tops: &[String]) {
    if let Ok(mut t) = TOPS.write() {
        t.extend(tops.iter().cloned());
    }
}

static SKIP_LIBRARIES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Reports nothing in library files (`-v`, `-y`): they are read only to
/// resolve instances. Off by default, as uvmcheck reports in them.
pub fn set_skip_libraries(on: bool) {
    SKIP_LIBRARIES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The tops `-top` named (empty: every module nothing instantiates).
pub(crate) fn tops() -> Vec<String> {
    TOPS.read().map(|t| t.clone()).unwrap_or_default()
}

/// Rules with no uvmcheck (C++) twin: dv-lint's own. `--list` marks them
/// `native`, and the parity gates leave them out.
pub const NATIVE_ONLY: &[&str] =
    &["UVM-API-003", "SV-RND-030", "SV-RND-031", "SV-RND-032", "SV-COV-018", "SV-RCE-002", "DPI-PRT-001"];

/// Ids a rule reports under besides its own (a rule engine's sub-rules).
pub fn sub_rules(id: &str) -> &'static [(&'static str, &'static str)] {
    match id {
        "SV-STY-100" => crate::support::style_config::STYLE_SUBS,
        "SV-NAM-100" => crate::support::style_config::NAMING_SUBS,
        _ => &[],
    }
}

/// Every ported rule, in the order uvmcheck registers them (by id).
pub fn rules() -> Vec<Rule> {
    let mut v = rules::all();
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
    lint_from(Source::Ready(design), trees, keep).1
}

/// [`lint_all`] for a design that `build` makes from `trees`: with several
/// threads, the caches the rules share start building on them while this
/// thread builds the design. Returns the design too.
pub fn lint_built(
    build: impl FnOnce() -> Design,
    trees: &[sv_syntax::Tree],
    keep: impl Fn(&str) -> bool,
) -> (Design, (Vec<String>, Vec<Finding>)) {
    let (d, out) = lint_from(Source::Build(Box::new(build)), trees, keep);
    (d.expect("a built design"), out)
}

/// The design rules run on: one given, or one to build.
pub(crate) enum Source<'d> {
    Ready(&'d Design),
    Build(Box<dyn FnOnce() -> Design + 'd>),
}

pub(crate) fn lint_from(
    design: Source<'_>,
    trees: &[sv_syntax::Tree],
    keep: impl Fn(&str) -> bool,
) -> (Option<Design>, (Vec<String>, Vec<Finding>)) {
    let all: Vec<Rule> = rules().into_iter().filter(|r| keep(r.id)).collect();
    let gen = shared::begin();
    let jobs = jobs().min(all.len());
    let built = std::sync::OnceLock::new();
    let given = match &design {
        Source::Ready(d) => Some(*d),
        Source::Build(_) => None,
    };
    let out = if jobs > 1 {
        let out = lint_parallel(design, &built, trees, &all, jobs, gen);
        let design = given.unwrap_or_else(|| built.get().expect("the design, once the rules ran"));
        out.unwrap_or_else(|| lint_serial(design, trees, &all))
    } else {
        let design = match design {
            Source::Ready(d) => d,
            Source::Build(b) => built.get_or_init(b),
        };
        lint_serial(design, trees, &all)
    };
    shared::end(gen);
    let (files, mut f) = out;
    // Nothing is reported in the test UVM mock, nor in library files (-v,
    // -y) after `--skip-libraries`; uvmcheck reports in libraries.
    let skip_libs = SKIP_LIBRARIES.load(std::sync::atomic::Ordering::Relaxed);
    let library: std::collections::HashSet<String> = trees
        .iter()
        .filter(|t| skip_libs && t.library)
        .flat_map(|t| t.files.iter().map(|p| p.display().to_string()))
        .collect();
    f.retain(|x| {
        let file = x.position_in(&files).0;
        !file.contains("uvm_mock.svh") && !library.contains(file)
    });
    dedup(&mut f, &files);
    INLINE_WAIVED.fetch_add(waivers::apply(&mut f, &files), std::sync::atomic::Ordering::Relaxed);
    f.sort_by(|a, b| (a.position_in(&files), a.rule).cmp(&(b.position_in(&files), b.rule)));
    (built.into_inner(), (files, f))
}

/// One finding per rule, position and message. A rule that walks the
/// elaborated design meets the same line once per module instance and class
/// specialization; the copies say nothing new, and their number depends on
/// how often (and in what order) the design is elaborated. uvmcheck merges
/// them the same way. The first copy, with its related locations, is kept.
fn dedup(f: &mut Vec<Finding>, files: &[String]) {
    let mut seen = std::collections::HashSet::new();
    f.retain(|x| seen.insert((x.position_in(files), x.rule, x.message.clone())));
}

/// Threads for the rules: `DV_LINT_JOBS`, else the machine's cores.
pub(crate) fn jobs() -> usize {
    std::env::var("DV_LINT_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
        .max(1)
}

/// The rules one after another: model rules, then each tree's syntax
/// rules, then the mixed rules over all trees.
pub(crate) fn lint_serial(design: &Design, trees: &[sv_syntax::Tree], all: &[Rule]) -> (Vec<String>, Vec<Finding>) {
    let mut sink = Sink::default();
    for r in all {
        if let Check::Model(run) = r.check {
            timed(r.id, || run(design, &mut sink));
        }
    }
    let mut locator = sv_sema::Locator::new(design.files().to_vec());
    let mixed = all.iter().any(|r| matches!(r.check, Check::Mixed(_)));
    if all.iter().any(Rule::is_syntax) {
        let mut prior: std::collections::HashSet<std::path::PathBuf> = Default::default();
        for (ix, tree) in trees.iter().enumerate() {
            let mut cst = syntax::Cst::new(tree, ix, locator);
            cst.prior_files = std::rc::Rc::new(prior.clone());
            cst.all = trees;
            prior.extend(tree.files.iter().cloned());
            for r in all {
                if let Check::Syntax(run) = r.check {
                    timed(r.id, || run(&cst, &mut sink));
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
        let csts = mixed_csts(trees, &files);
        for r in all {
            if let Check::Mixed(run) = r.check {
                timed(r.id, || run(design, &csts, &mut sink));
            }
        }
    }
    (files, sink.findings)
}

/// Runs `f`, printing its time under `DV_LINT_PROFILE` (serial runs).
pub(crate) fn timed(id: &str, f: impl FnOnce()) {
    if std::env::var_os("DV_LINT_PROFILE").is_none() {
        return f();
    }
    let t = std::time::Instant::now();
    f();
    eprintln!("profile {id} {:.1}", t.elapsed().as_secs_f64() * 1e3);
}

/// The trees as the mixed rules see them, sharing file table `files`.
pub(crate) fn mixed_csts<'t>(trees: &'t [sv_syntax::Tree], files: &[String]) -> Vec<syntax::Cst<'t>> {
    trees
        .iter()
        .enumerate()
        .map(|(ix, t)| {
            let mut c = syntax::Cst::new(t, ix, sv_sema::Locator::new(files.to_vec()));
            c.all = trees;
            c
        })
        .collect()
}

/// The trees as the syntax rules see them (each knowing the files of the
/// trees before it), sharing file table `files`.
pub(crate) fn syntax_csts<'t>(trees: &'t [sv_syntax::Tree], files: &[String]) -> Vec<syntax::Cst<'t>> {
    let mut prior: std::collections::HashSet<std::path::PathBuf> = Default::default();
    let mut out = Vec::new();
    for (ix, tree) in trees.iter().enumerate() {
        let mut c = syntax::Cst::new(tree, ix, sv_sema::Locator::new(files.to_vec()));
        c.prior_files = std::rc::Rc::new(prior.clone());
        c.all = trees;
        prior.extend(tree.files.iter().cloned());
        out.push(c);
    }
    out
}

/// A unit of parallel work: build a shared cache, or run a rule.
pub(crate) enum Task {
    Warm(fn(&syntax::Cst)),
    Rule(usize),
}

/// Rules that take longest on large designs, started first so that they
/// do not finish last.
pub(crate) const SLOW: &[&str] = &[
    "SV-STY-010",
    "SV-BUG-001",
    "SV-CMP-001",
    "SV-LANG-003",
    "SV-CNC-003",
    "SV-DED-004",
    "SV-DED-001",
    "SV-DED-012",
    "SV-PKG-003",
    "SV-PRF-007",
    "SV-API-001",
    "SV-LANG-002",
    "SV-RDB-006",
];

/// The rules on `jobs` threads, each taking the next rule; the caches in
/// [`shared`] are built once for all of them. Every thread numbers files
/// from one table made up front; `None` (run serially instead) if a thread
/// had to number a file that table lacks.
pub(crate) fn lint_parallel<'d>(
    source: Source<'d>,
    built: &'d std::sync::OnceLock<Design>,
    trees: &[sv_syntax::Tree],
    all: &[Rule],
    jobs: usize,
    gen: usize,
) -> Option<(Vec<String>, Vec<Finding>)> {
    // The design and the file table, once this thread has them; the
    // workers build shared caches until then.
    // `Some(None)`: building the design failed (its panic is resumed).
    type Ready<'a> = Option<Option<(&'a Design, std::sync::Arc<Vec<String>>)>>;
    let ready: (std::sync::Mutex<Ready>, std::sync::Condvar) = Default::default();
    let wait = || {
        let mut g = ready.0.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(r) = &*g {
                return r.clone();
            }
            g = ready.1.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    };
    let publish = |design: &'d Design| {
        let mut locator = sv_sema::Locator::new(design.files().to_vec());
        for (ix, tree) in trees.iter().enumerate() {
            let cst = syntax::Cst::new(tree, ix, locator);
            for f in 0..tree.files.len() as u32 {
                cst.loc_at(sv_syntax::Origin { file: f, offset: 0 });
            }
            locator = cst.into_locator();
        }
        let files = std::sync::Arc::new(locator.into_files());
        *ready.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Some((design, files.clone())));
        ready.1.notify_all();
        files
    };
    // The work, in order: first the caches most rules wait for (when many
    // rules run), then the slowest rules, then the rest as registered.
    let warm: &[fn(&syntax::Cst)] = if all.len() >= 32 {
        &[
            |c| drop(crate::expr_types::expr_types(c)),
            |c| drop(shared::sema_checks(c)),
            |c| drop(shared::elaboration(c)),
            |c| {
                let _ = c.spec_count(sv_syntax::NodeId(0));
            },
        ]
    } else {
        &[]
    };
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by_key(|&i| SLOW.iter().position(|&s| s == all[i].id).unwrap_or(SLOW.len()));
    let tasks: Vec<Task> = warm.iter().map(|&w| Task::Warm(w)).chain(order.into_iter().map(Task::Rule)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let found: Vec<std::sync::Mutex<Vec<Finding>>> = all.iter().map(|_| Default::default()).collect();
    let same_files = std::sync::atomic::AtomicBool::new(true);
    let profile = std::env::var_os("DV_LINT_PROFILE").is_some();
    let t0 = std::time::Instant::now();
    let files = std::thread::scope(|s| {
        for job in 0..jobs {
            let (next, found, same_files, tasks, t0, wait) = (&next, &found, &same_files, &tasks, &t0, &wait);
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, move || {
                    shared::enter(gen);
                    let mut syn: Option<Vec<syntax::Cst>> = None;
                    let mut mix: Option<Vec<syntax::Cst>> = None;
                    let mut files: Option<std::sync::Arc<Vec<String>>> = None;
                    let mut design: Option<&Design> = None;
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let i = match tasks.get(k) {
                            None => break,
                            Some(Task::Warm(w)) => {
                                let a = t0.elapsed().as_secs_f64() * 1e3;
                                // The caches do not depend on the file table.
                                if let Some(t) = trees.first() {
                                    let mut c = syntax::Cst::new(t, 0, sv_sema::Locator::default());
                                    c.all = trees;
                                    w(&c);
                                }
                                if profile {
                                    eprintln!("ptask warm{k} {job} {a:.0} {:.0}", t0.elapsed().as_secs_f64() * 1e3);
                                }
                                continue;
                            }
                            Some(&Task::Rule(i)) => i,
                        };
                        let r = &all[i];
                        let a = t0.elapsed().as_secs_f64() * 1e3;
                        if design.is_none() {
                            let Some((d, f)) = wait() else { break };
                            design = Some(d);
                            files = Some(f);
                        }
                        let (Some(design), Some(files)) = (design, files.as_deref()) else { break };
                        let mut sink = Sink::default();
                        match r.check {
                            Check::Model(run) => run(design, &mut sink),
                            Check::Syntax(run) => {
                                for c in syn.get_or_insert_with(|| syntax_csts(trees, files)).iter() {
                                    run(c, &mut sink);
                                }
                            }
                            Check::Mixed(run) => {
                                run(design, mix.get_or_insert_with(|| mixed_csts(trees, files)), &mut sink)
                            }
                        }
                        if profile {
                            eprintln!("ptask {} {job} {a:.0} {:.0}", r.id, t0.elapsed().as_secs_f64() * 1e3);
                        }
                        if let Ok(mut f) = found[i].lock() {
                            *f = sink.findings;
                        }
                    }
                    let n = files.map_or(0, |f| f.len());
                    for c in syn.into_iter().chain(mix).flatten() {
                        if c.into_locator().into_files().len() != n {
                            same_files.store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                })
                .expect("spawn a rule thread");
        }
        let design = match source {
            Source::Ready(d) => d,
            Source::Build(b) => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| built.get_or_init(b))) {
                Ok(d) => d,
                Err(e) => {
                    *ready.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
                    ready.1.notify_all();
                    std::panic::resume_unwind(e)
                }
            },
        };
        publish(design)
    });
    let files = std::sync::Arc::try_unwrap(files).unwrap_or_else(|f| (*f).clone());
    if !same_files.into_inner() {
        return None;
    }
    // In the serial order: model rules, then syntax, then mixed.
    let mut out = Vec::new();
    let mut found: Vec<Vec<Finding>> = found.into_iter().map(|m| m.into_inner().unwrap_or_default()).collect();
    for group in 0..3 {
        for (i, r) in all.iter().enumerate() {
            let g = match r.check {
                Check::Model(_) => 0,
                Check::Syntax(_) => 1,
                Check::Mixed(_) => 2,
            };
            if g == group {
                out.append(&mut found[i]);
            }
        }
    }
    Some((files, out))
}

/// Visits of each generic class's body, one line per class, by the syntax
/// engine and by the model: `<syntax count> <name>@<file>:<line> <model count>`.
pub fn spec_report(design: &Design, trees: &[sv_syntax::Tree]) -> Vec<String> {
    let mut locator = sv_sema::Locator::new(design.files().to_vec());
    let mut rows = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let cst = syntax::Cst::new(t, ix, locator);
        for (node, k) in crate::support::specializations::spec_counts(&cst) {
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
    let base =
        |f: u32| files.get(f as usize).map(|p| p.rsplit('/').next().unwrap_or("").to_string()).unwrap_or_default();
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
