//! The native semantic layer: everything above the syntax tree that slang
//! used to provide. It reads `sv-syntax` trees and gives three things:
//!
//! - **The class model.** [`load`] takes compile options (`-f`, `+incdir+`,
//!   `+define+`, files, `--single-unit`) and returns a [`dv_model::Design`]
//!   like `Design::load` does: packages, `$unit`, nested and parameterized
//!   classes (with the specializations the design names), typedefs,
//!   imports, base classes, UVM roles, properties, methods with their
//!   bodies (calls, assignments, timing controls, `super.new`), arguments,
//!   locals, constraints and type strings as slang prints them.
//!   [`load_with_trees`] also hands back the trees, for syntax rules.
//! - **Elaboration.** [`elaborate_full`] builds the instance tree from the
//!   top modules: instances, generate blocks taken, parameter values.
//! - **Semantic checks.** [`SemaChecks`] runs the checks a compiler makes
//!   (the `check` function of each public module below) and
//!   [`semantic_diags`] merges them with elaboration's, which is what
//!   `dv-build` reports.
//!
//! Rules that need more than the model read it here too: [`oracle`] gives
//! the declared type of any name for expression typing, [`SpecModel`] and
//! [`spec_counts`] answer per-specialization questions, and [`Locator`]
//! turns token positions into the model's [`Loc`]s.
//!
//! Inside, a `Builder` collects every declaration into scopes
//! ([`scopes`]), resolves names with constant evaluation (`consteval`,
//! `packed`, [`bigint`]) and fills the model (`types` for type strings,
//! `body` for method bodies); `elab` elaborates on the same builder.

use crate::fx::{FxMap as HashMap, FxSet as HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use sv_source::{Dialect, FileList, ProcessEnv};

use dv_model::{
    Argument, Class, ClassId, Constraint, Design, Direction, Error, Loc, Local, MacroUse, Method, PhaseKind, Property,
    Timings, TypeShape, Typedef, UvmKind, Visibility,
};
use sv_syntax::{NodeId, Options, SyntaxKind, Tree};

pub mod bigint;
pub mod bind;
mod body;
pub mod calls;
pub mod checks;
pub mod classes;
mod consteval;
mod elab;
pub mod fx;
pub mod iclass;
pub mod library;
pub mod names;
pub mod oracle;
pub mod order;
mod packed;
pub mod scopes;
pub mod sva;
pub mod typed;
mod types;
pub mod udp;
use SyntaxKind::*;

/// Loads and models a design from compile options, as `Design::load` does.
pub fn load<S: AsRef<str>>(args: &[S]) -> Result<Design, Error> {
    load_with_trees(args).map(|(d, _)| d)
}

/// [`load`], also giving back the parsed trees (for syntax-level tools).
pub fn load_with_trees<S: AsRef<str>>(args: &[S]) -> Result<(Design, Vec<Tree>), Error> {
    let (trees, parse_ms) = parse_args(args)?;
    let design = design_of(&trees, parse_ms);
    Ok((design, trees))
}

/// The parse half of [`load_with_trees`]: the trees, and the time taken (ms).
pub fn parse_args<S: AsRef<str>>(args: &[S]) -> Result<(Vec<Tree>, f64), Error> {
    // The options are read as a compiler reads them: -f/-F file lists,
    // $VAR/${VAR} from the environment, +incdir+, +define+, -I, -D.
    // `--single-unit` reads every file into one compilation unit and tree.
    let single = args.iter().any(|a| a.as_ref() == "--single-unit");
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let list = FileList::read_args(
        args.iter().map(|a| a.as_ref()).filter(|a| *a != "--single-unit"),
        &cwd,
        Dialect::Any,
        &ProcessEnv,
    );
    let opts = Options {
        include_dirs: list.include_dirs.iter().map(|d| d.value.clone()).collect(),
        defines: list
            .effective_defines()
            .into_iter()
            .map(|(n, v)| match v {
                Some(v) => format!("{n}={v}"),
                None => n,
            })
            .collect(),
        ..Options::default()
    };
    let files: Vec<PathBuf> = list.sources.iter().map(|s| s.value.clone()).collect();
    if files.is_empty() {
        return Err(Error("no sources given".into()));
    }
    let t0 = Instant::now();
    for f in &files {
        if !f.exists() {
            return Err(Error(format!("could not read the sources: {}", f.display())));
        }
    }
    let mut trees = if single {
        vec![sv_syntax::parse_unit(&files, opts.clone())]
    } else {
        sv_syntax::parse_files(&files, opts.clone())
    };
    // -v files and -y directories resolve what the sources instantiate.
    library::add_library_trees(&mut trees, &list, &opts);
    let parse_ms = t0.elapsed().as_secs_f64() * 1e3;
    Ok((trees, parse_ms))
}

/// The model half of [`load_with_trees`]: the design of parsed trees.
pub fn design_of(trees: &[Tree], parse_ms: f64) -> Design {
    let errors = trees.iter().map(|t| t.errors.len() + t.pp_diagnostics.len()).sum::<usize>() as u32;
    let t1 = Instant::now();
    let design = build(trees);
    let model_ms = t1.elapsed().as_secs_f64() * 1e3;
    let counts = design.specialization_counts().to_vec();
    let mut design = Design::from_parts(
        design.files().to_vec(),
        design.classes().to_vec(),
        errors,
        Timings { parse_ms, elaborate_ms: 0.0, model_ms, copy_ms: 0.0 },
    );
    design.set_specializations(counts);
    design
}

/// Turns token positions of parsed trees into [`Loc`]s, numbering files as
/// they are first seen (index 0 is "no file", as in slang's model). A
/// locator seeded with a design's files numbers them as that design does.
pub struct Locator {
    files: Vec<String>,
    file_ids: crate::fx::FxMap<(usize, u32), u32>,
    /// Byte offset of each line start, per (tree, preprocessor file).
    line_starts: crate::fx::FxMap<(usize, u32), Vec<u32>>,
}

impl Default for Locator {
    fn default() -> Self {
        Locator::new(vec![String::new()])
    }
}

impl Locator {
    /// A locator with `files` as its file table (start it with `[""]` so 0
    /// stays "no file").
    pub fn new(files: Vec<String>) -> Self {
        Locator { files, file_ids: Default::default(), line_starts: Default::default() }
    }

    /// File names by index, the table [`Loc::file`] indexes.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The file table, for [`dv_model::Design::from_parts`].
    pub fn into_files(self) -> Vec<String> {
        self.files
    }

    /// Location of token `tok` of `tree` (index `tree_ix` among the trees
    /// this locator sees). Code from a macro is placed where its text was
    /// written, as slang's model does, and also remembers the expansion
    /// site, where findings on it are reported.
    pub fn loc(&mut self, tree_ix: usize, tree: &Tree, tok: u32) -> Loc {
        let Some(token) = tree.tokens.get(tok as usize) else {
            return Loc::default();
        };
        let (written, origin, from_macro) = (token.written, token.origin, token.from_macro);
        let (file, line, column) = self.position(tree_ix, tree, written);
        let expansion = from_macro.then(|| {
            let (f, l, _) = self.position(tree_ix, tree, origin);
            (f, l)
        });
        Loc { file, line, column, expansion }
    }

    /// Design file index, line and column of a preprocessor origin.
    pub fn position(&mut self, tree_ix: usize, t: &Tree, o: sv_syntax::Origin) -> (u32, u32, u32) {
        let key = (tree_ix, o.file);
        let file = match self.file_ids.get(&key) {
            Some(&f) => f,
            None => {
                let name = t.files[o.file as usize].to_string_lossy().into_owned();
                let f = match self.files.iter().position(|n| *n == name) {
                    Some(i) => i as u32,
                    None => {
                        self.files.push(name);
                        (self.files.len() - 1) as u32
                    }
                };
                self.file_ids.insert(key, f);
                f
            }
        };
        let src = &t.sources[o.file as usize];
        let off = (o.offset as usize).min(src.len()) as u32;
        let starts = self.line_starts.entry(key).or_insert_with(|| {
            std::iter::once(0)
                .chain(src.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i as u32 + 1))
                .collect()
        });
        // The last line start at or before the offset.
        let line = starts.partition_point(|&s| s <= off);
        (file, line as u32, off - starts[line - 1] + 1)
    }
}

/// The type of each value and typedef of the scopes (`None` for other
/// symbols), as slang prints it: builder phase N2.
pub fn symbol_types(trees: &[Tree], s: &scopes::Scopes) -> Vec<Option<String>> {
    symbol_info(trees, s).0
}

/// The elaborated design as `uvmcheck_sym_dump --hier` prints it (phase
/// N4): instances, taken generate blocks and parameter values by path.
pub fn hierarchy(trees: &[Tree]) -> Vec<String> {
    elaborate(trees).0
}

/// The hierarchy (as [`hierarchy`]) and the errors elaboration finds.
pub fn elaborate(trees: &[Tree]) -> (Vec<String>, Vec<names::Diag>) {
    elaborate_tops(trees, &[])
}

/// [`elaborate`] from the given top modules (`-top`); none: every module
/// nothing instantiates.
pub fn elaborate_tops(trees: &[Tree], tops: &[String]) -> (Vec<String>, Vec<names::Diag>) {
    let e = elaborate_full(trees, tops);
    (e.hierarchy, e.diags)
}

/// What elaboration finds: the hierarchy lines, errors, and the nodes no
/// instance elaborates (whose name errors slang does not report).
pub struct Elaborated {
    /// Instances, taken generate blocks and parameter values, one line each, in
    /// the form `uvmcheck_sym_dump --hier` prints.
    pub hierarchy: Vec<String>,
    /// Errors only elaboration finds (they depend on parameter values or on
    /// how an instance is connected).
    pub diags: Vec<names::Diag>,
    /// Definition bodies no instance elaborates, by (tree, node).
    pub skipped: Vec<(usize, sv_syntax::NodeId)>,
    /// Copies of each definition body and generate item (by tree and
    /// node): all of them, and those instantiated (see `Elab::copies`).
    pub copies: HashMap<(usize, sv_syntax::NodeId), (usize, usize)>,
}

/// Every semantic error the native front end finds in a compile (what
/// `dv-build` reports after the syntax errors), with the hierarchy it
/// elaborated.
pub fn semantic_diags(trees: &[Tree], tops: &[String]) -> (Vec<names::Diag>, Elaborated) {
    let parts = SemaChecks::new(trees);
    let elab = elaborate_full(trees, tops);
    (parts.diags(trees, &elab), elab)
}

/// The semantic checks of a compile, each on its own, so that callers
/// that need some of them (and elaboration separately) compute them once.
pub struct SemaChecks {
    /// What [`names::check`] found.
    pub names: Vec<names::Diag>,
    /// What [`checks::check`] found.
    pub checks: Vec<names::Diag>,
    /// What [`bind::check`] found.
    pub bind: Vec<names::Diag>,
    /// What [`iclass::check`] found.
    pub iclass: Vec<names::Diag>,
    /// What [`udp::check`] found.
    pub udp: Vec<names::Diag>,
    /// What [`sva::check`] found.
    pub sva: Vec<names::Diag>,
    /// What [`calls::check`] found.
    pub calls: Vec<names::Diag>,
    /// What [`order::check`] found.
    pub order: Vec<names::Diag>,
    /// What [`classes::check`] found.
    pub classes: Vec<names::Diag>,
    /// What [`typed::check`] found.
    pub typed: Vec<names::Diag>,
}

impl SemaChecks {
    /// Builds the scopes once and runs every check on them, the slowest on
    /// threads of their own.
    pub fn new(trees: &[Tree]) -> SemaChecks {
        let scopes = scopes::build(trees);
        // The checks are independent: the slowest run on threads of their own.
        std::thread::scope(|s| {
            let big = || std::thread::Builder::new().stack_size(256 << 20);
            let scopes = &scopes;
            let typed = big().spawn_scoped(s, move || typed::check(trees, scopes)).expect("spawn a check thread");
            let rest = big()
                .spawn_scoped(s, move || (classes::check(trees), bind::check(trees), order::check(trees, scopes)))
                .expect("spawn a check thread");
            let names = names::check(trees, scopes);
            let checks = checks::check(trees, scopes);
            let iclass = iclass::check(trees);
            let udp = udp::check(trees);
            let sva = sva::check(trees);
            let calls = calls::check(trees);
            let (classes, bind, order) = rest.join().unwrap_or_else(|e| std::panic::resume_unwind(e));
            let typed = typed.join().unwrap_or_else(|e| std::panic::resume_unwind(e));
            SemaChecks { names, checks, bind, iclass, udp, sva, calls, order, classes, typed }
        })
    }

    /// Every semantic error, in [`semantic_diags`]'s order, given the
    /// compile's elaboration.
    pub fn diags(&self, trees: &[Tree], elab: &Elaborated) -> Vec<names::Diag> {
        let mut out = self.names.clone();
        out.extend(self.checks.iter().cloned());
        out.extend(elab.diags.iter().cloned());
        // Names in what no instance elaborates are not checked (as in slang).
        let skipped: Vec<(usize, u32, u32)> =
            elab.skipped.iter().filter_map(|&(tr, n)| trees[tr].token_bounds(n).map(|(a, b)| (tr, a, b))).collect();
        out.extend(
            self.bind
                .iter()
                .filter(|d| {
                    let Some((a, _)) = trees[d.tree].token_bounds(d.node) else { return true };
                    !skipped.iter().any(|&(tr, s, e)| tr == d.tree && s <= a && a <= e)
                })
                .cloned(),
        );
        for v in [&self.iclass, &self.udp, &self.sva, &self.calls, &self.order, &self.classes, &self.typed] {
            out.extend(v.iter().cloned());
        }
        out
    }
}

/// Collects every declaration of `trees` and elaborates from `tops` (none:
/// every module nothing instantiates), keeping everything [`Elaborated`] holds.
pub fn elaborate_full(trees: &[Tree], tops: &[String]) -> Elaborated {
    let mut b = Builder::new(trees);
    b.designs = true;
    b.collect_names();
    b.collect();
    let mut e = elab::Elab::new(&b);
    e.top = tops.to_vec();
    e.run();
    let skipped = e.skipped.iter().map(|r| (r.tree, r.node)).collect();
    let copies = e.copies.iter().map(|(r, k)| ((r.tree, r.node), *k)).collect();
    Elaborated { hierarchy: e.out, diags: e.diags, skipped, copies }
}

/// Types (as [`symbol_types`]) and the values of parameters, specparams
/// and enum values as slang prints them (phase N3); `None` where a symbol
/// has none or it is not evaluated.
pub fn symbol_info(trees: &[Tree], s: &scopes::Scopes) -> (Vec<Option<String>>, Vec<Option<String>>) {
    use scopes::SymKind as K;
    let mut b = Builder::new(trees);
    b.designs = true;
    b.collect_names();
    b.collect();
    let empty: Env = Rc::new(HashMap::default());
    // The builder scope of each scope: subroutines read their parent's.
    let mut map = vec![b.unit; s.scopes.len()];
    for (i, sc) in s.scopes.iter().enumerate() {
        let parent = sc.parent.map_or(b.unit, |p| map[p]);
        map[i] = match sc.symbol {
            Some(y) => {
                let y = &s.symbols[y];
                b.node_scope.get(&NodeRef { tree: y.tree, node: y.node }).copied().unwrap_or(parent)
            }
            None => b.unit,
        };
    }
    let mut nettypes: HashMap<String, NodeRef> = HashMap::default();
    for (ti, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if t.kind(n) == SyntaxKind::NetTypeDecl {
                if let Some(nm) = t.child(n, SyntaxKind::Name) {
                    nettypes.insert(t.bare_text(nm), NodeRef { tree: ti, node: n });
                }
            }
        }
    }
    // A class's members are typed in its default specialization: each
    // type parameter bound to its default.
    let mut envs: HashMap<usize, Env> = HashMap::default();
    for c in &b.classes {
        let mut m = HashMap::default();
        for (name, is_type, default) in &c.params {
            if let (true, Some(d)) = (is_type, default) {
                m.insert(name.clone(), Bound { node: *d, scope: c.parent, env: empty.clone() });
            }
        }
        envs.insert(c.scope, Rc::new(m));
    }
    // Not inside a parameterized class (see tools/oracles/sym_dump.cpp).
    let in_generic = |scope: usize| {
        let mut cur = Some(scope);
        while let Some(c) = cur {
            if let ScopeKind::Class(k) = b.scopes[c].kind {
                if b.classes[k].generic {
                    return true;
                }
            }
            cur = b.scopes[c].parent;
        }
        false
    };
    let values = s
        .symbols
        .iter()
        .map(|y| {
            if !matches!(y.kind, K::Parameter | K::Specparam | K::EnumValue) || in_generic(map[y.scope]) {
                return None;
            }
            let scope = map[y.scope];
            let env = envs.get(&scope).cloned().unwrap_or_else(|| empty.clone());
            b.symbol_value(y, scope, &env)
        })
        .collect();
    let types = s
        .symbols
        .iter()
        .map(|y| {
            if !matches!(y.kind, K::Net | K::Variable | K::Parameter | K::Argument | K::Typedef) {
                return None;
            }
            let scope = map[y.scope];
            if in_generic(scope) {
                return None;
            }
            let env = envs.get(&scope).cloned().unwrap_or_else(|| empty.clone());
            let r = |n: NodeId| NodeRef { tree: y.tree, node: n };
            if y.kind == K::Typedef {
                return y.item.map(|t| b.typedef_target(r(t), scope, &env));
            }
            let t = &trees[y.tree];
            if y.kind == K::Variable && t.kind(y.node) == SyntaxKind::CovergroupDecl {
                return Some("<unnamed covergroup>".into());
            }
            // A net of a user net type has that type's data type.
            if let Some(dt) = y.dtype {
                if let Some(&nt) = nettypes.get(t.bare_text(dt).as_str()) {
                    let ndt = b.child(nt, SyntaxKind::DataType);
                    return Some(b.type_string(ndt, None, scope, &env, true));
                }
            }
            if y.kind == K::Parameter {
                let value = y
                    .item
                    .and_then(|d| t.children(d).find(|&c| !matches!(t.kind(c), SyntaxKind::Name | SyntaxKind::Dim)));
                if value.is_none() && y.dtype.is_none() && matches!(b.scopes[scope].kind, ScopeKind::Class(_)) {
                    // A class parameter with no default: slang's invalid specialization.
                    return Some("<error>".into());
                }
                if y.dtype.is_none() {
                    if let Some(v) = value {
                        return Some(b.value_type(r(v), scope, &env));
                    }
                }
            }
            Some(b.type_string(y.dtype.map(r), y.item.map(r), scope, &env, false))
        })
        .collect();
    (types, values)
}

/// Models already parsed trees (one per compilation unit).
/// How many specializations slang elaborates for each parameterized
/// class, the default one included, by (tree, class declaration node).
/// Classes in modules, interfaces and programs count too.
pub fn spec_counts(trees: &[Tree]) -> HashMap<(usize, sv_syntax::NodeId), usize> {
    let mut b = Builder::new(trees);
    b.designs = true;
    b.collect_names();
    b.collect();
    b.spec_counts.iter().map(|(&c, &n)| ((b.classes[c].node.tree, b.classes[c].node.node), n)).collect()
}

/// The specializations slang elaborates (see [`spec_counts`]) with their
/// bindings, for rules that look at what a type names in each of them.
pub struct SpecModel<'t> {
    b: Builder<'t>,
    regions: Vec<ScopeMap>,
}

impl<'t> SpecModel<'t> {
    /// Collects the declarations of `trees` and the specializations of every
    /// generic class.
    pub fn new(trees: &'t [Tree]) -> Self {
        let mut b = Builder::new(trees);
        b.designs = true;
        b.collect_names();
        b.collect();
        let regions = (0..trees.len()).map(|t| b.scope_map(t)).collect();
        SpecModel { b, regions }
    }

    /// The class declaration (tree, node) a type node names, once per
    /// specialization of the generic class around it (once outside one);
    /// `None` where it names no class.
    pub fn class_per_spec(&self, tree: usize, dt: sv_syntax::NodeId) -> Vec<Option<(usize, sv_syntax::NodeId)>> {
        self.per_spec(tree, dt, false)
    }

    /// Like [`Self::class_per_spec`], over only the specializations the
    /// design names, where slang reports diagnostics.
    pub fn class_per_named_spec(&self, tree: usize, dt: sv_syntax::NodeId) -> Vec<Option<(usize, sv_syntax::NodeId)>> {
        self.per_spec(tree, dt, true)
    }

    fn per_spec(&self, tree: usize, dt: sv_syntax::NodeId, named: bool) -> Vec<Option<(usize, sv_syntax::NodeId)>> {
        let b = &self.b;
        let r = NodeRef { tree, node: dt };
        let Some((first, _)) = b.trees[tree].token_bounds(dt) else {
            return Vec::new();
        };
        let scope = Builder::scope_at(&self.regions[tree], first, b.unit);
        let envs: Vec<Env> = match b.generic_owner(scope) {
            None => vec![Env::default()],
            Some(g) => if named { &b.named_sets } else { &b.spec_sets }
                .get(&g)
                .map(|m| m.values().map(|v| v.1.clone()).collect())
                .unwrap_or_default(),
        };
        envs.iter()
            .map(|env| match b.resolve_env(r, scope, env, 0).resolved {
                Resolved::Class(c) => Some((b.classes[c].node.tree, b.classes[c].node.node)),
                _ => None,
            })
            .collect()
    }

    /// How many specializations the generic class around a node has (1
    /// outside one).
    pub fn visits(&self, tree: usize, n: sv_syntax::NodeId) -> usize {
        let b = &self.b;
        let Some((first, _)) = b.trees[tree].token_bounds(n) else {
            return 1;
        };
        let scope = Builder::scope_at(&self.regions[tree], first, b.unit);
        match b.generic_owner(scope) {
            None => 1,
            Some(g) => b.spec_counts.get(&g).copied().unwrap_or(0),
        }
    }
}

/// The class model of parsed trees, with every file numbered as it is first
/// seen. [`load`] wraps this with option handling, parsing and timings.
pub fn build(trees: &[Tree]) -> Design {
    let mut b = Builder::new(trees);
    b.collect_names();
    b.collect();
    b.finish()
}

// ---- symbol tables ----------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct NodeRef {
    tree: usize,
    node: NodeId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Unit,
    Package,
    Class(usize),
    /// A module, interface or program (only with `Builder::designs`).
    Design,
}

struct Scope {
    kind: ScopeKind,
    name: String,
    parent: Option<usize>,
    classes: crate::fx::FxMap<String, usize>,
    typedefs: crate::fx::FxMap<String, NodeRef>,
    /// `import p::*` (None) and `import p::x` (Some).
    imports: Vec<(String, Option<String>)>,
    /// Type parameters of a class, with their default type.
    type_params: crate::fx::FxMap<String, Option<NodeRef>>,
    /// Variables and subroutines of a package or the unit.
    values: crate::fx::FxMap<String, Member>,
    /// Parameters and localparams (value ones): data type and value.
    consts: crate::fx::FxMap<String, (Option<NodeRef>, Option<NodeRef>)>,
}

struct ClassDef {
    name: String,
    node: NodeRef,
    /// Scope the class is declared in.
    parent: usize,
    /// The class's own scope (members, nested classes, typedefs).
    scope: usize,
    generic: bool,
    extends: Option<NodeRef>,
    /// Parameters in order: name, whether a type, default.
    params: Vec<(String, bool, Option<NodeRef>)>,
    /// Properties and methods by name.
    members: crate::fx::FxMap<String, Member>,
}

/// A named value or subroutine declared in a class or package.
#[derive(Clone, Copy)]
enum Member {
    /// A variable: its DataDecl and Declarator.
    Var { decl: NodeRef, item: NodeRef },
    /// A function or task: its declaration (or prototype).
    Sub { decl: NodeRef },
}

/// Actual type parameters of a specialization, by name.
type Env = std::rc::Rc<HashMap<String, Bound>>;

/// A class's methods by name: first declaration `virtual`, any `virtual`.
type MethodDecls<'t> = Rc<HashMap<&'t str, (bool, bool)>>;

/// A type argument, resolved where it was written.
#[derive(Clone)]
struct Bound {
    node: NodeRef,
    scope: usize,
    env: Env,
}

/// What a type name stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Resolved {
    Class(usize),
    Vif,
    Str,
    Enum,
    Event,
    /// A built-in class of package std: process, semaphore, mailbox.
    Std(&'static str),
    Other,
}

struct TypeInfo {
    resolved: Resolved,
    shape: TypeShape,
    /// For a class: its type arguments.
    args: Env,
}

/// Each token's innermost scope region (the smallest region around it,
/// the first listed on a tie); `u32::MAX`: none.
pub(crate) struct ScopeMap {
    by_tok: Vec<u32>,
}

impl ScopeMap {
    fn new(regions: &[(u32, u32, usize)], ntok: usize) -> ScopeMap {
        let mut order: Vec<usize> = (0..regions.len()).collect();
        // Larger first, and on a tie the later first, so that the region
        // painted last is the one the old scan chose.
        order.sort_by(|&a, &b| {
            let (la, lb) = (regions[a].1 - regions[a].0, regions[b].1 - regions[b].0);
            lb.cmp(&la).then(b.cmp(&a))
        });
        let mut by_tok = vec![u32::MAX; ntok];
        for i in order {
            let (lo, hi, s) = regions[i];
            let hi = (hi as usize).min(ntok.saturating_sub(1));
            for t in lo as usize..=hi {
                if t < ntok {
                    by_tok[t] = s as u32;
                }
            }
        }
        ScopeMap { by_tok }
    }
}

type SpecSets = HashMap<usize, std::collections::BTreeMap<String, (String, Env)>>;

struct SpecUse {
    r: NodeRef,
    scope: usize,
    head: String,
    owner: Option<usize>,
    dependent: Option<usize>,
}

struct Builder<'t> {
    trees: &'t [Tree],
    scopes: Vec<Scope>,
    packages: crate::fx::FxMap<String, usize>,
    unit: usize,
    classes: Vec<ClassDef>,
    /// Out-of-block bodies: (class name, method name) -> body.
    bodies: HashMap<(String, String), NodeRef>,
    /// Names used as types anywhere (generic classes with a use have members).
    used_names: crate::fx::FxSet<String>,
    /// Names declared as types and as subroutines anywhere: a use of
    /// anything else is an error to slang.
    type_names: crate::fx::FxSet<String>,
    sub_names: crate::fx::FxSet<String>,
    /// Interface declarations by name (virtual interface types).
    interfaces: crate::fx::FxMap<String, NodeRef>,
    /// The specialization slang reads a parameterized class's members
    /// from: the one that prints first, as its type arguments.
    rep_env: HashMap<usize, Env>,
    /// Set while [`Self::class_key`] prints: class types print canonically.
    keying: std::cell::Cell<bool>,
    /// Each node's parent, per tree, built on first use.
    parent_of: std::cell::OnceCell<Vec<Vec<u32>>>,
    /// Values of parameters outside class specializations, by scope.
    const_cache: std::cell::RefCell<fx::FxMap<(usize, String), Option<types::CVal>>>,
    /// Literal values by node.
    lit_cache: std::cell::RefCell<fx::FxMap<NodeRef, Option<types::CVal>>>,
    /// Scope -> the members of the enum typedefs declared in it, first wins.
    enum_cache: std::cell::RefCell<HashMap<usize, std::rc::Rc<HashMap<String, types::CVal>>>>,
    /// Scope -> member name -> the enum typedef declared in it that names it, first wins.
    enum_typedefs: std::cell::RefCell<HashMap<usize, std::rc::Rc<HashMap<String, NodeRef>>>>,
    /// Enum type strings outside any specialization, by (tree, node, scope, alias).
    enum_strings: std::cell::RefCell<HashMap<(usize, NodeId, usize, Option<String>), String>>,
    /// Typedefs declared in blocks and routines, by name: tree, the
    /// block's token bounds, the declaration's first token, the decl.
    block_typedefs: HashMap<String, Vec<(usize, u32, u32, u32, NodeRef)>>,
    /// Specializations slang elaborates per generic class (default included).
    spec_counts: HashMap<usize, usize>,
    /// Those specializations' bindings, keyed by identity.
    spec_sets: SpecSets,
    /// Only the specializations the design names (slang suppresses
    /// diagnostics in default ones and in what they elaborate).
    named_sets: SpecSets,
    /// Scope of each class and package node.
    node_scope: crate::fx::FxMap<NodeRef, usize>,
    locator: Locator,
    /// `base_env` of each class with no bindings, as lookups ask it often.
    base_cache: std::cell::RefCell<HashMap<usize, Option<(usize, Env)>>>,
    chain_cache: std::cell::RefCell<HashMap<usize, Rc<Vec<usize>>>>,
    /// Per class, see [`Builder::method_decls`].
    decl_cache: std::cell::RefCell<HashMap<usize, MethodDecls<'t>>>,
    /// Also make scopes for modules, interfaces and programs (the builder's
    /// typing, phase N2); the class model leaves them out.
    designs: bool,
    /// Also take the localparams of generate blocks as their design's
    /// (the first of a name; for typing expressions in them).
    block_consts: bool,
    /// Locals of the constant functions being run (see `consteval`).
    frames: std::cell::RefCell<Vec<HashMap<String, types::CVal>>>,
    /// The declared types of those locals (with "" for the return value).
    ftypes: std::cell::RefCell<Vec<HashMap<String, packed::Ty>>>,
    /// Set when an evaluation met an invalid constant (a zero replication
    /// or a cast to zero width): slang leaves such a value `<unset>`.
    invalid: std::cell::Cell<bool>,
    /// Parameters and genvars of the instance being elaborated (see `elab`).
    /// `None`: a parameter with no value here (its declaration's must not be used).
    inst: std::cell::RefCell<elab::Vals>,
}

impl<'t> Builder<'t> {
    fn new(trees: &'t [Tree]) -> Self {
        let unit = Scope {
            kind: ScopeKind::Unit,
            name: String::new(),
            parent: None,
            classes: Default::default(),
            typedefs: Default::default(),
            imports: Vec::new(),
            type_params: Default::default(),
            values: Default::default(),
            consts: Default::default(),
        };
        Builder {
            trees,
            scopes: vec![unit],
            packages: Default::default(),
            unit: 0,
            classes: Vec::new(),
            bodies: HashMap::default(),
            used_names: Default::default(),
            type_names: Default::default(),
            sub_names: Default::default(),
            interfaces: Default::default(),
            rep_env: HashMap::default(),
            keying: Default::default(),
            parent_of: Default::default(),
            const_cache: Default::default(),
            lit_cache: Default::default(),
            enum_cache: Default::default(),
            enum_typedefs: Default::default(),
            enum_strings: Default::default(),
            block_typedefs: HashMap::default(),
            spec_counts: HashMap::default(),
            spec_sets: HashMap::default(),
            named_sets: HashMap::default(),
            node_scope: Default::default(),
            locator: Locator::default(),
            base_cache: Default::default(),
            chain_cache: Default::default(),
            decl_cache: Default::default(),
            designs: false,
            block_consts: false,
            frames: Default::default(),
            ftypes: Default::default(),
            invalid: Default::default(),
            inst: Default::default(),
        }
    }

    fn collect_names(&mut self) {
        use sv_syntax::SyntaxKind::*;
        for (ti, t) in self.trees.iter().enumerate() {
            for id in t.preorder() {
                let r = NodeRef { tree: ti, node: id };
                let k = t.kind(id);
                if k == InterfaceDecl {
                    if let Some(n) = t.child(id, Name) {
                        if let Some(&tok) = t.token_range(n).last() {
                            self.interfaces.entry(t.tokens[tok as usize].text.to_string()).or_insert(r);
                        }
                    }
                }
                let set = match k {
                    ClassDecl | TypedefDecl | ForwardTypedef | InterfaceDecl | ModuleDecl | ProgramDecl
                    | CovergroupDecl | PackageDecl | NetTypeDecl | CheckerDecl => &mut self.type_names,
                    FunctionDecl | TaskDecl | MethodPrototype | DpiImport => &mut self.sub_names,
                    _ => continue,
                };
                if let Some(n) = t.child(id, Name) {
                    if let Some(&tok) = t.token_range(n).last() {
                        set.insert(t.tokens[tok as usize].text.to_string());
                    }
                }
                let _ = r;
            }
            // `type T` parameters.
            for w in t.tokens.windows(2) {
                if &*w[0].text == "type" && w[1].kind == sv_syntax::TokenKind::Identifier {
                    self.type_names.insert(w[1].text.to_string());
                }
            }
        }
    }

    fn tree(&self, r: NodeRef) -> &'t Tree {
        &self.trees[r.tree]
    }

    fn kind(&self, r: NodeRef) -> SyntaxKind {
        self.tree(r).kind(r.node)
    }

    fn child(&self, r: NodeRef, k: SyntaxKind) -> Option<NodeRef> {
        self.tree(r).child(r.node, k).map(|node| NodeRef { tree: r.tree, node })
    }

    fn children(&self, r: NodeRef) -> Vec<NodeRef> {
        self.tree(r).children(r.node).map(|node| NodeRef { tree: r.tree, node }).collect()
    }

    /// The node's own tokens (not its children's), in order.
    fn own_tokens(&self, r: NodeRef) -> Vec<&'t str> {
        let t = self.tree(r);
        t.node(r.node)
            .children
            .iter()
            .filter_map(|c| match *c {
                sv_syntax::Child::Token(i) => Some(&*t.tokens[i as usize].text),
                _ => None,
            })
            .collect()
    }

    /// The node's first own token (see [`Self::own_tokens`]).
    fn first_own_tok(&self, r: NodeRef) -> Option<&'t str> {
        let t = self.tree(r);
        t.node(r.node).children.iter().find_map(|c| match *c {
            sv_syntax::Child::Token(i) => Some(&*t.tokens[i as usize].text),
            _ => None,
        })
    }

    fn tokens(&self, r: NodeRef) -> Vec<u32> {
        self.tree(r).token_range(r.node)
    }

    /// The node's only token, if it has exactly one.
    fn single_tok(&self, r: NodeRef) -> Option<&'t str> {
        match self.tree(r).token_bounds(r.node)? {
            (a, b) if a == b => Some(self.text(r.tree, a)),
            _ => None,
        }
    }

    fn text(&self, tree: usize, tok: u32) -> &'t str {
        &self.trees[tree].tokens[tok as usize].text
    }

    /// Last token of a Name node: the method of `C::f`, or the plain name.
    fn name_of(&self, r: NodeRef) -> Option<(String, u32)> {
        self.name_str(r).map(|(n, t)| (n.to_string(), t))
    }

    /// [`Self::name_of`] without copying the name.
    fn name_str(&self, r: NodeRef) -> Option<(&'t str, u32)> {
        let n = self.child(r, Name)?;
        let (_, t) = self.tree(n).token_bounds(n.node)?;
        Some((self.text(r.tree, t), t))
    }

    fn loc(&mut self, tree: usize, tok: u32) -> Loc {
        self.locator.loc(tree, &self.trees[tree], tok)
    }

    fn position(&mut self, tree: usize, o: sv_syntax::Origin) -> (u32, u32, u32) {
        self.locator.position(tree, &self.trees[tree], o)
    }

    fn new_scope(&mut self, kind: ScopeKind, name: &str, parent: Option<usize>) -> usize {
        self.scopes.push(Scope {
            kind,
            name: name.to_string(),
            parent,
            classes: Default::default(),
            typedefs: Default::default(),
            imports: Vec::new(),
            type_params: Default::default(),
            values: Default::default(),
            consts: Default::default(),
        });
        self.scopes.len() - 1
    }

    // ---- collection -----------------------------------------------------

    fn collect(&mut self) {
        for i in 0..self.trees.len() {
            let root = NodeRef { tree: i, node: self.trees[i].root };
            self.collect_scope(root, self.unit);
        }
        self.specialized_generics();
        self.representatives();
    }

    fn collect_scope(&mut self, r: NodeRef, scope: usize) {
        for c in self.children(r) {
            match self.kind(c) {
                PackageDecl => {
                    let Some((name, _)) = self.name_of(c) else {
                        continue;
                    };
                    let s = match self.packages.get(&name) {
                        Some(&s) => s,
                        None => {
                            let s = self.new_scope(ScopeKind::Package, &name, Some(self.unit));
                            self.packages.insert(name.clone(), s);
                            s
                        }
                    };
                    self.node_scope.insert(c, s);
                    self.collect_scope(c, s);
                }
                ClassDecl => self.collect_class(c, scope),
                ModuleDecl | InterfaceDecl | ProgramDecl if self.designs => {
                    let Some((name, _)) = self.name_of(c) else {
                        continue;
                    };
                    let s = self.new_scope(ScopeKind::Design, &name, Some(scope));
                    self.node_scope.insert(c, s);
                    if let Some(h) = self.child(c, Header) {
                        // `module m import p::*; #(...)`
                        for i in self.children(h) {
                            if self.kind(i) == ImportDecl {
                                self.collect_import(i, s);
                            }
                        }
                    }
                    if let Some(pl) = self.child(c, Header).and_then(|h| self.child(h, ParamList)) {
                        self.collect_design_params(pl, s);
                    }
                    self.collect_scope(c, s);
                }
                GenerateRegion if self.designs => self.collect_scope(c, scope),
                IfGenerate | LoopGenerate | CaseGenerate | GenerateBlock | ElseClause
                    if self.designs && self.block_consts =>
                {
                    self.collect_block_consts(c, scope)
                }
                TypedefDecl => {
                    if let Some((name, _)) = self.name_of(c) {
                        self.scopes[scope].typedefs.insert(name, c);
                    }
                }
                ImportDecl => self.collect_import(c, scope),
                ParamDecl if self.designs && self.own_tokens(c).contains(&"type") => self.collect_type_params(c, scope),
                ParamDecl => self.collect_consts(c, scope),
                FunctionDecl | TaskDecl => {
                    // `function T C::f(...)`: an out-of-block body.
                    if let Some(n) = self.child(c, Name) {
                        let toks = self.tokens(n);
                        if toks.len() >= 3 {
                            let class = self.text(c.tree, toks[toks.len() - 3]).to_string();
                            let method = self.text(c.tree, toks[toks.len() - 1]).to_string();
                            self.bodies.entry((class, method)).or_insert(c);
                        } else if let Some(&t) = toks.last() {
                            let name = self.text(c.tree, t).to_string();
                            self.scopes[scope].values.entry(name).or_insert(Member::Sub { decl: c });
                        }
                    }
                }
                DpiImport => {
                    if let Some((name, _)) = self.name_of(c) {
                        self.scopes[scope].values.entry(name).or_insert(Member::Sub { decl: c });
                    }
                }
                DataDecl => {
                    for item in self.children(c) {
                        if self.kind(item) == Declarator {
                            if let Some((name, _)) = self.name_of(item) {
                                self.scopes[scope].values.entry(name).or_insert(Member::Var { decl: c, item });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `#(parameter type T = int, U = bit, int N = 1)` of a design unit.
    fn collect_block_consts(&mut self, r: NodeRef, scope: usize) {
        for c in self.children(r) {
            match self.kind(c) {
                IfGenerate | LoopGenerate | CaseGenerate | GenerateBlock | ElseClause | GenerateRegion => {
                    self.collect_block_consts(c, scope)
                }
                ParamDecl if !self.own_tokens(c).contains(&"type") => {
                    let fresh = self
                        .children(c)
                        .into_iter()
                        .filter(|d| self.kind(*d) == Declarator)
                        .all(|d| self.name_of(d).is_some_and(|n| !self.scopes[scope].consts.contains_key(&n.0)));
                    if fresh {
                        self.collect_consts(c, scope);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_design_params(&mut self, pl: NodeRef, s: usize) {
        let mut ty = false;
        for p in self.children(pl) {
            if self.kind(p) != ParamDecl {
                continue;
            }
            let own = self.own_tokens(p);
            if own.contains(&"type") {
                ty = true;
            } else if own.iter().any(|t| matches!(*t, "parameter" | "localparam")) || self.child(p, DataType).is_some()
            {
                ty = false;
            }
            if !ty {
                self.collect_consts(p, s);
                continue;
            }
            self.collect_type_params(p, s);
        }
    }

    /// The names of a `type` parameter declaration, with their defaults.
    fn collect_type_params(&mut self, p: NodeRef, s: usize) {
        {
            if let Some(n) = self.child(p, Name) {
                if let Some((name, _)) = self.tokens(n).last().map(|&t| (self.text(n.tree, t).to_string(), t)) {
                    let d = self.child(p, DataType);
                    self.scopes[s].type_params.insert(name, d);
                }
            }
            for d in self.children(p) {
                if self.kind(d) == Declarator {
                    if let Some((name, _)) = self.name_of(d) {
                        let v = self.children(d).into_iter().find(|c| !matches!(self.kind(*c), Name | Dim));
                        self.scopes[s].type_params.insert(name, v);
                    }
                }
            }
        }
    }

    /// `localparam int A = 1, B = A + 1;`
    fn collect_consts(&mut self, r: NodeRef, scope: usize) {
        if self.own_tokens(r).contains(&"type") {
            return;
        }
        let dt = self.child(r, DataType);
        for d in self.children(r) {
            if self.kind(d) != Declarator {
                continue;
            }
            if let Some((n, _)) = self.name_of(d) {
                let v = self.children(d).into_iter().find(|c| !matches!(self.kind(*c), Name | Dim));
                self.scopes[scope].consts.entry(n).or_insert((dt, v));
            }
        }
    }

    fn collect_import(&mut self, r: NodeRef, scope: usize) {
        let toks: Vec<&str> = self.tokens(r).iter().map(|&t| self.text(r.tree, t)).collect();
        // import a::*, b::x;
        let mut i = 1;
        while i + 2 < toks.len() {
            if toks[i + 1] == "::" {
                let item = if toks[i + 2] == "*" { None } else { Some(toks[i + 2].to_string()) };
                self.scopes[scope].imports.push((toks[i].to_string(), item));
                i += 3;
            } else {
                i += 1;
            }
            if i < toks.len() && toks[i] == "," {
                i += 1;
            }
        }
    }

    fn collect_class(&mut self, r: NodeRef, parent: usize) {
        let Some((name, _)) = self.name_of(r) else {
            return;
        };
        let idx = self.classes.len();
        let own = self.new_scope(ScopeKind::Class(idx), &name, Some(parent));
        let header = self.child(r, Header);
        let mut generic = false;
        let mut extends = None;
        let mut params = Vec::new();
        if let Some(h) = header {
            if let Some(pl) = self.child(h, ParamList) {
                generic = true;
                // `type` carries over to the following parameters until a
                // data type or `parameter` says otherwise.
                let mut sticky_type = false;
                for p in self.children(pl) {
                    if self.kind(p) != ParamDecl {
                        continue;
                    }
                    let own_toks = self.own_tokens(p);
                    if own_toks.contains(&"type") {
                        sticky_type = true;
                    } else if own_toks.iter().any(|t| matches!(*t, "parameter" | "localparam"))
                        || self.child(p, DataType).is_some()
                    {
                        sticky_type = false;
                    }
                    let (pname, default) = if let Some(n) = self.child(p, Name) {
                        (self.tokens(n).last().map(|&t| self.text(n.tree, t).to_string()), self.child(p, DataType))
                    } else if let Some(d) = self.child(p, Declarator) {
                        let n = self.name_of(d).map(|(n, _)| n);
                        let v = self.children(d).into_iter().find(|c| !matches!(self.kind(*c), Name | Dim));
                        (n, v)
                    } else {
                        (None, None)
                    };
                    let Some(pname) = pname else { continue };
                    if sticky_type {
                        self.scopes[own].type_params.insert(pname.clone(), default);
                    } else {
                        let dt = self.child(p, DataType);
                        self.scopes[own].consts.insert(pname.clone(), (dt, default));
                    }
                    params.push((pname, sticky_type, default));
                }
            }
            // `implements` lists share the Extends node kind; only the
            // `extends` clause names a base class.
            let base = self
                .children(h)
                .into_iter()
                .find(|&e| self.tree(e).node(e.node).kind == Extends && self.own_tokens(e).first() == Some(&"extends"));
            if let Some(e) = base {
                extends = self.child(e, DataType);
            }
        }
        let mut members = crate::fx::FxMap::default();
        for m in self.children(r) {
            match self.kind(m) {
                DataDecl => {
                    for item in self.children(m) {
                        if self.kind(item) == Declarator {
                            if let Some((n, _)) = self.name_of(item) {
                                members.entry(n).or_insert(Member::Var { decl: m, item });
                            }
                        }
                    }
                }
                FunctionDecl | TaskDecl | MethodPrototype => {
                    if let Some((n, _)) = self.name_of(m) {
                        members.entry(n).or_insert(Member::Sub { decl: m });
                    }
                }
                // An embedded covergroup is also a property of its type.
                CovergroupDecl => {
                    if let Some((n, _)) = self.name_of(m) {
                        members.entry(n).or_insert(Member::Var { decl: m, item: m });
                    }
                }
                _ => {}
            }
        }
        self.classes.push(ClassDef {
            name: name.clone(),
            node: r,
            parent,
            scope: own,
            generic,
            extends,
            params,
            members,
        });
        self.scopes[parent].classes.entry(name).or_insert(idx);
        self.node_scope.insert(r, own);
        for c in self.children(r) {
            match self.kind(c) {
                ClassDecl => self.collect_class(c, own),
                TypedefDecl => {
                    if let Some((n, _)) = self.name_of(c) {
                        self.scopes[own].typedefs.insert(n, c);
                    }
                }
                ParamDecl => self.collect_consts(c, own),
                _ => {}
            }
        }
    }

    /// Identifiers used outside the name of their own declaration, each
    /// with the parameterized class whose body it is in (if any).
    fn collect_name_uses(&self, tree: usize, generics: &HashSet<&str>, uses: &mut HashSet<(String, Option<String>)>) {
        let t = &self.trees[tree];
        let mut own = HashSet::default();
        // Token ranges of generic class bodies (and their out-of-block
        // methods), innermost last after sorting by start.
        let mut regions: Vec<(u32, u32, String)> = Vec::new();
        for n in t.preorder() {
            let k = t.kind(n);
            match k {
                ClassDecl | ForwardTypedef | EndLabel | FunctionDecl | TaskDecl => {
                    if let Some(name) = t.child(n, Name) {
                        own.extend(t.token_range(name));
                    }
                    if k == EndLabel {
                        own.extend(t.token_range(n));
                    }
                }
                _ => {}
            }
            let owner = match k {
                ClassDecl => t.child(n, Name).and_then(|x| t.token_range(x).last().copied()),
                FunctionDecl | TaskDecl => t.child(n, Name).and_then(|x| {
                    let r = t.token_range(x);
                    (r.len() >= 3).then(|| r[r.len() - 3])
                }),
                _ => None,
            };
            if let Some(o) = owner {
                let name = &*t.tokens[o as usize].text;
                if generics.contains(name) {
                    let r = t.token_range(n);
                    if let (Some(&lo), Some(&hi)) = (r.first(), r.last()) {
                        regions.push((lo, hi, name.to_string()));
                    }
                }
            }
        }
        for (i, tok) in t.tokens.iter().enumerate() {
            let i = i as u32;
            if tok.kind != sv_syntax::TokenKind::Identifier || own.contains(&i) || !generics.contains(&*tok.text) {
                continue;
            }
            // The innermost generic region holding the use.
            let owner = regions
                .iter()
                .filter(|(lo, hi, _)| *lo <= i && i <= *hi)
                .min_by_key(|(lo, hi, _)| hi - lo)
                .map(|(_, _, n)| n.clone());
            uses.insert((tok.text.to_string(), owner));
        }
    }

    /// Parameterized classes something specializes: used from code that is
    /// itself elaborated (anything outside an unspecialized generic class).
    fn specialized_generics(&mut self) {
        let generics: HashSet<&str> = self.classes.iter().filter(|c| c.generic).map(|c| c.name.as_str()).collect();
        let mut uses = HashSet::default();
        for i in 0..self.trees.len() {
            self.collect_name_uses(i, &generics, &mut uses);
        }
        let mut live: crate::fx::FxSet<String> = Default::default();
        loop {
            let before = live.len();
            for (name, owner) in &uses {
                if owner.as_ref().map_or(true, |o| live.contains(o) && o != name) {
                    live.insert(name.clone());
                }
            }
            if live.len() == before {
                break;
            }
        }
        self.used_names = live;
    }

    /// Innermost scope around each token range: classes, packages and the
    /// out-of-block bodies of class methods.
    fn regions(&self, tree: usize) -> Vec<(u32, u32, usize)> {
        let t = &self.trees[tree];
        let mut out = Vec::new();
        let mut stack = vec![(t.root, self.unit)];
        while let Some((n, scope)) = stack.pop() {
            let r = NodeRef { tree, node: n };
            let mut inner = scope;
            if let Some(&s) = self.node_scope.get(&r) {
                inner = s;
            } else if matches!(t.kind(n), FunctionDecl | TaskDecl) {
                // `function T C::f` belongs to class C.
                if let Some(name) = t.child(n, Name) {
                    let toks = t.token_range(name);
                    if toks.len() >= 3 {
                        let class = &*t.tokens[toks[toks.len() - 3] as usize].text;
                        if let Some(Found::Class(c)) = self.lookup(class, scope, 0) {
                            inner = self.classes[c].scope;
                        }
                    }
                }
            }
            if inner != scope {
                let r = t.token_range(n);
                if let (Some(&lo), Some(&hi)) = (r.first(), r.last()) {
                    out.push((lo, hi, inner));
                }
            }
            if matches!(t.kind(n), ClassDecl | PackageDecl | SourceFile | FunctionDecl | TaskDecl) || inner != scope {
                for c in t.children(n) {
                    stack.push((c, inner));
                }
            }
        }
        out
    }

    fn scope_at(regions: &ScopeMap, tok: u32, unit: usize) -> usize {
        match regions.by_tok.get(tok as usize) {
            Some(&s) if s != u32::MAX => s as usize,
            _ => unit,
        }
    }

    /// [`Self::regions`] indexed by token.
    fn scope_map(&self, tree: usize) -> ScopeMap {
        ScopeMap::new(&self.regions(tree), self.trees[tree].tokens.len())
    }

    /// The generic class whose (unspecialized) body holds `scope`, if any.
    fn generic_owner(&self, scope: usize) -> Option<usize> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let ScopeKind::Class(c) = self.scopes[s].kind {
                if self.classes[c].generic {
                    return Some(c);
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// For each specialized generic class, the specialization that prints
    /// first (slang reads the definition's members from that one).
    fn representatives(&mut self) {
        self.collect_block_typedefs();
        // Every specialization by its printed form. Uses inside a generic
        // class that depend on its parameters specialize once per
        // specialization of that class, so this runs to a fixpoint.
        // Keyed by the canonical form (bindings resolved through typedefs
        // and parameters), holding the printed form and the bindings.
        let mut uses = Vec::new();
        let generic: crate::fx::FxSet<&str> =
            self.classes.iter().filter(|c| c.generic).map(|c| c.name.as_str()).collect();
        for tree in 0..self.trees.len() {
            let regions = self.scope_map(tree);
            let t = &self.trees[tree];
            for n in t.preorder() {
                if !matches!(t.kind(n), DataType | NameExpr) {
                    continue;
                }
                let r = NodeRef { tree, node: n };
                let Some((first, _)) = t.token_bounds(n) else { continue };
                if t.tokens[first as usize].kind != sv_syntax::TokenKind::Identifier {
                    continue;
                }
                let toks = t.token_range(n);
                // Only a class named right here specializes it (not one a
                // typedef or parameter stands for).
                let head = toks.iter().take_while(|&&k| &*t.tokens[k as usize].text != "#").last().unwrap_or(&first);
                let head = &*t.tokens[*head as usize].text;
                if !generic.contains(head) {
                    continue;
                }
                let scope = Self::scope_at(&regions, first, self.unit);
                let owner = self.generic_owner(scope);
                // The enclosing generic class (innermost first) whose
                // parameters the use depends on.
                let mut dependent = None;
                let mut cur = owner;
                while let Some(g) = cur {
                    if self.uses_type_param(r, g) {
                        dependent = Some(g);
                        break;
                    }
                    cur = self.scopes[self.classes[g].parent]
                        .parent
                        .and_then(|_| self.generic_owner(self.classes[g].parent));
                }
                uses.push(SpecUse { r, scope, head: head.to_string(), owner, dependent });
            }
        }
        // Representatives come from the named specializations only.
        let specs = self.spec_fixpoint(&uses, false);
        // The engine also elaborates each definition's default
        // specialization, and the uses inside it, so counts run the
        // fixpoint again seeded with the defaults.
        let full = self.spec_fixpoint(&uses, true);
        if let Ok(only) = std::env::var("SV_SPECS_LIST") {
            for (c, m) in &full {
                if self.classes[*c].name == only {
                    for (k, (shown, _)) in m {
                        eprintln!("{k}  =>  {shown}");
                    }
                }
            }
        }
        for c in 0..self.classes.len() {
            if self.classes[c].generic {
                self.spec_counts.insert(c, full.get(&c).map_or(0, |m| m.len()));
            }
        }
        self.spec_sets = full;
        self.named_sets = specs.clone();
        self.rep_env = specs
            .into_iter()
            .filter_map(|(c, m)| m.into_values().min_by(|a, b| a.0.cmp(&b.0)).map(|(_, env)| (c, env)))
            .collect();
        self.chain_cache.borrow_mut().clear();
    }

    fn collect_block_typedefs(&mut self) {
        let mut out: HashMap<String, Vec<(usize, u32, u32, u32, NodeRef)>> = HashMap::default();
        for (ti, t) in self.trees.iter().enumerate() {
            let mut stack = vec![t.root];
            while let Some(n) = stack.pop() {
                let in_block = !matches!(t.kind(n), ClassDecl | PackageDecl | ModuleDecl | InterfaceDecl | ProgramDecl)
                    && n != t.root;
                for c in t.children(n) {
                    if in_block && t.kind(c) == TypedefDecl {
                        let (Some(nm), Some((lo, hi)), Some((at, _))) =
                            (t.child(c, Name), t.token_bounds(n), t.token_bounds(c))
                        else {
                            continue;
                        };
                        out.entry(t.bare_text(nm)).or_default().push((ti, lo, hi, at, NodeRef { tree: ti, node: c }));
                    }
                    stack.push(c);
                }
            }
        }
        self.block_typedefs = out;
    }

    /// A typedef declared earlier in a block around `at`, innermost first.
    pub(crate) fn block_typedef(&self, name: &str, at: NodeRef) -> Option<NodeRef> {
        let first = self.trees[at.tree].token_bounds(at.node)?.0;
        self.block_typedefs
            .get(name)?
            .iter()
            .filter(|(t, lo, hi, decl, _)| *t == at.tree && *lo <= first && first <= *hi && *decl < first)
            .min_by_key(|(_, lo, hi, _, _)| hi - lo)
            .map(|x| x.4)
    }

    /// `env` with every unbound parameter of `c` bound to its default; a
    /// default sees the parameters before it (`type RSP = REQ`).
    fn complete_env(&self, c: usize, env: &Env) -> Env {
        let mut full: HashMap<String, Bound> = (**env).clone();
        for (name, _, d) in &self.classes[c].params {
            if full.contains_key(name) {
                continue;
            }
            if let Some(d) = d {
                let prev = std::rc::Rc::new(full.clone());
                full.insert(name.clone(), Bound { node: *d, scope: self.classes[c].scope, env: prev });
            }
        }
        std::rc::Rc::new(full)
    }

    /// Specializations reachable from the uses, to a fixpoint, keyed by
    /// the canonical form and holding the printed form and bindings. With
    /// `defaults`, every generic's default specialization is a seed and
    /// code in any generic with one is elaborated.
    fn spec_fixpoint(&self, uses: &[SpecUse], defaults: bool) -> SpecSets {
        let mut specs: SpecSets = HashMap::default();
        // Each class's specialization keys in the order they were added
        // (entries are only ever added).
        let mut order: crate::fx::FxMap<usize, Vec<String>> = Default::default();
        if defaults {
            for c in 0..self.classes.len() {
                if self.classes[c].generic && self.classes[c].params.iter().all(|p| p.2.is_some()) {
                    let env = self.complete_env(c, &Env::default());
                    let key = self.class_key(c, &env);
                    let shown = self.print_class(c, &env, 0);
                    let m = specs.entry(c).or_default();
                    if !m.contains_key(&key) {
                        order.entry(c).or_default().push(key.clone());
                    }
                    m.insert(key, (shown, env));
                }
            }
        }
        // Whether each class's name is used (code inside an unspecialized
        // generic is never elaborated).
        let used: Vec<bool> = self.classes.iter().map(|k| self.used_names.contains(&k.name)).collect();
        // The (use, specialization) pairs already resolved: resolving one
        // again gives the same key, so only new specializations are visited:
        // a use has seen the first `seen[ui]` keys of its class's `order`
        // (and its one environment when it depends on none).
        let mut seen: Vec<usize> = vec![0; uses.len()];
        for _round in 0..32 {
            let mut changed = false;
            for (ui, u) in uses.iter().enumerate() {
                if let Some(g) = u.owner {
                    let live = used[g] || (defaults && specs.contains_key(&g));
                    if !live {
                        continue;
                    }
                }
                let envs: Vec<(String, Env)> = match u.dependent {
                    None if seen[ui] == 0 => {
                        seen[ui] = 1;
                        vec![(String::new(), Env::default())]
                    }
                    None => Vec::new(),
                    Some(g) => match (specs.get(&g), order.get(&g)) {
                        (Some(m), Some(o)) if o.len() > seen[ui] => {
                            // In key order, as the map lists them.
                            let mut new: Vec<(String, Env)> =
                                o[seen[ui]..].iter().map(|k| (k.clone(), m[k].1.clone())).collect();
                            new.sort_by(|a, b| a.0.cmp(&b.0));
                            seen[ui] = o.len();
                            new
                        }
                        _ => Vec::new(),
                    },
                };
                for (_ek, env) in envs {
                    let mut info = self.resolve_env(u.r, u.scope, &env, 0);
                    // `C #(..)::member` specializes C.
                    let named = matches!(info.resolved, Resolved::Class(c) if self.classes[c].name == u.head);
                    if defaults && !named && self.child(u.r, ParamList).is_some() {
                        let toks = self.own_tokens(u.r);
                        let at = toks.iter().position(|t| *t == u.head.as_str());
                        let path: Vec<&str> =
                            toks[..at.map_or(0, |a| a + 1)].iter().copied().filter(|t| *t != "::").collect();
                        let found = match path.len() {
                            0 => None,
                            1 => self.lookup(path[0], u.scope, 0),
                            _ => self.lookup_path(&path, u.scope, 0),
                        };
                        if let Some(Found::Class(c)) = found {
                            info.args = self.bind_args(c, u.r, u.scope, &env);
                            info.resolved = Resolved::Class(c);
                        }
                    }
                    let Resolved::Class(c) = info.resolved else {
                        continue;
                    };
                    if self.classes[c].name != u.head || !self.classes[c].generic {
                        continue;
                    }
                    let key = self.class_key(c, &info.args);
                    let m = specs.entry(c).or_default();
                    if m.len() < 256 && !m.contains_key(&key) {
                        order.entry(c).or_default().push(key.clone());
                        let shown = self.print_class(c, &info.args, 0);
                        let env = if defaults { self.complete_env(c, &info.args) } else { info.args };
                        m.insert(key, (shown, env));
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        specs
    }

    /// Whether a type use depends on a parameter of generic class `g`,
    /// directly or through typedefs and type arguments.
    fn uses_type_param(&self, r: NodeRef, g: usize) -> bool {
        let scope = {
            // Resolve from inside g, where its parameters are visible.
            self.classes[g].scope
        };
        self.depends(r, scope, g, 0)
    }

    fn depends(&self, r: NodeRef, scope: usize, g: usize, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        let gscope = self.classes[g].scope;
        let toks: Vec<&str> = self.tokens(r).iter().map(|&t| self.text(r.tree, t)).collect();
        // Every identifier in the use (names, arguments, dimensions).
        for (k, name) in toks.iter().enumerate() {
            if k > 0 && toks[k - 1] == "::" {
                continue;
            }
            if self.classes[g].params.iter().any(|p| p.0 == *name) {
                return true;
            }
            match self.lookup(name, scope, depth + 1) {
                Some(Found::Param(_, s)) if s == gscope => return true,
                Some(Found::Typedef(t, s)) if self.is_within(s, gscope) => {
                    if let Some(dt) = self.child(t, DataType) {
                        if self.depends(dt, s, g, depth + 1) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// `s` is `outer` or nested in it.
    fn is_within(&self, s: usize, outer: usize) -> bool {
        let mut cur = Some(s);
        while let Some(x) = cur {
            if x == outer {
                return true;
            }
            cur = self.scopes[x].parent;
        }
        false
    }

    /// A specialization as slang prints it: `name#(arg,...)`.
    fn print_class(&self, c: usize, env: &Env, depth: usize) -> String {
        self.class_string(c, env, depth)
    }

    // ---- lookup -----------------------------------------------------------

    /// A name seen from `scope`: a class, a typedef (with the scope it lives
    /// in) or nothing.
    fn lookup(&self, name: &str, scope: usize, depth: usize) -> Option<Found> {
        if depth > 32 {
            return None;
        }
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let Some(f) = self.lookup_in(name, s, depth) {
                return Some(f);
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// Declared in `s`, inherited into it (classes) or imported into it.
    fn lookup_in(&self, name: &str, s: usize, depth: usize) -> Option<Found> {
        let sc = &self.scopes[s];
        if let Some(d) = sc.type_params.get(name) {
            return Some(Found::Param(*d, s));
        }
        if let Some(&c) = sc.classes.get(name) {
            return Some(Found::Class(c));
        }
        if let Some(&t) = sc.typedefs.get(name) {
            return Some(Found::Typedef(t, s));
        }
        if let ScopeKind::Class(c) = sc.kind {
            // Inherited typedefs, nested classes and parameters.
            if let Some(b) = self.base_of(c, depth + 1) {
                let bs = self.classes[b].scope;
                if bs != s {
                    if let Some(f) = self.lookup_in(name, bs, depth + 1) {
                        return Some(f);
                    }
                }
            }
        }
        for (pkg, item) in &sc.imports {
            if item.as_deref().is_some_and(|i| i != name) {
                continue;
            }
            if let Some(&p) = self.packages.get(pkg) {
                if p != s {
                    let ps = &self.scopes[p];
                    if let Some(&c) = ps.classes.get(name) {
                        return Some(Found::Class(c));
                    }
                    if let Some(&t) = ps.typedefs.get(name) {
                        return Some(Found::Typedef(t, p));
                    }
                }
            }
        }
        None
    }

    fn base_of(&self, c: usize, depth: usize) -> Option<usize> {
        self.base_env(c, &Env::default(), depth).map(|(b, _)| b)
    }

    /// The base class of `c` specialized with `env`, and the base's own
    /// type arguments.
    fn base_env(&self, c: usize, env: &Env, depth: usize) -> Option<(usize, Env)> {
        if depth > 32 {
            return None;
        }
        if env.is_empty() {
            if let Some(hit) = self.base_cache.borrow().get(&c) {
                return hit.clone();
            }
            let r = self.base_env_uncached(c, env, depth);
            self.base_cache.borrow_mut().insert(c, r.clone());
            return r;
        }
        self.base_env_uncached(c, env, depth)
    }

    fn base_env_uncached(&self, c: usize, env: &Env, depth: usize) -> Option<(usize, Env)> {
        let dt = self.classes[c].extends?;
        let parent = self.classes[c].parent;
        // The class's own type parameters are visible in its extends clause
        // (`extends BASE`); nothing else of its scope is.
        let first = self.tokens(dt).first().map(|&t| self.text(dt.tree, t)).unwrap_or("");
        let own = self.classes[c].scope;
        // The extends clause sees the class's own parameters: bound by
        // `env`, else their defaults.
        let mut full = (**env).clone();
        for (name, is_type, default) in &self.classes[c].params {
            if *is_type && !full.contains_key(name) {
                if let Some(d) = default {
                    // A default sees the parameters before it.
                    let prev = std::rc::Rc::new(full.clone());
                    full.insert(name.clone(), Bound { node: *d, scope: parent, env: prev });
                }
            }
        }
        let env = &std::rc::Rc::new(full);
        let info = if let Some(b) = env.get(first) {
            self.resolve_env(b.node, b.scope, &b.env, depth + 1)
        } else if self.scopes[own].type_params.contains_key(first) {
            return None;
        } else {
            self.resolve_env(dt, parent, env, depth + 1)
        };
        match info.resolved {
            Resolved::Class(b) if b != c => Some((b, info.args)),
            _ => None,
        }
    }

    /// `c` and its bases, with type arguments followed through.
    fn chain(&self, c: usize) -> Vec<usize> {
        let mut out = vec![c];
        let mut cur = (c, self.rep_env.get(&c).cloned().unwrap_or_default());
        while out.len() <= self.classes.len() {
            let Some(next) = self.base_env(cur.0, &cur.1, 0) else {
                break;
            };
            out.push(next.0);
            cur = next;
        }
        out
    }

    /// Resolves a type (a DataType, or a name expression standing for a
    /// type) seen from `scope`, with type parameters bound by `env`.
    fn resolve_env(&self, dt: NodeRef, scope: usize, env: &Env, depth: usize) -> TypeInfo {
        let other = TypeInfo { resolved: Resolved::Other, shape: TypeShape::Other, args: Env::default() };
        if depth > 32 {
            return other;
        }
        let toks: Vec<&str> = self.tokens(dt).iter().map(|&t| self.text(dt.tree, t)).collect();
        let Some(&first) = toks.first() else {
            return other;
        };
        if first == "virtual" || first == "interface" {
            return TypeInfo { resolved: Resolved::Vif, ..other };
        }
        if matches!(first, "struct" | "union") {
            let packed = toks.get(1).is_some_and(|t| *t == "packed") || toks.get(2).is_some_and(|t| *t == "packed");
            let shape = if packed { TypeShape::Other } else { TypeShape::UnpackedStruct };
            return TypeInfo { shape, ..other };
        }
        if self.trees[dt.tree].tokens[self.tokens(dt)[0] as usize].kind != sv_syntax::TokenKind::Identifier {
            let resolved = match first {
                "string" => Resolved::Str,
                "event" => Resolved::Event,
                "enum" => Resolved::Enum,
                _ => Resolved::Other,
            };
            return TypeInfo { resolved, ..other };
        }
        // `a::b::c`, stopping at `#(` and dimensions.
        let mut path = vec![first];
        let mut i = 1;
        loop {
            // `C#(...)::T`: the arguments do not name anything here.
            let mut j = i;
            if toks.get(j) == Some(&"#") && toks.get(j + 1) == Some(&"(") {
                let mut depth = 0;
                j += 1;
                while j < toks.len() {
                    match toks[j] {
                        "(" => depth += 1,
                        ")" => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            if j + 1 < toks.len() && toks[j] == "::" {
                path.push(toks[j + 1]);
                i = j + 2;
            } else {
                break;
            }
        }
        if path.len() == 1 {
            if let Some(b) = env.get(first) {
                return self.resolve_env(b.node, b.scope, &b.env, depth + 1);
            }
        }
        let found = if path.len() == 1 {
            self.lookup(path[0], scope, depth + 1)
        } else {
            self.lookup_path(&path, scope, depth + 1)
        };
        match found {
            Some(Found::Class(c)) => TypeInfo {
                resolved: Resolved::Class(c),
                shape: TypeShape::Other,
                args: self.bind_args(c, dt, scope, env),
            },
            Some(Found::Typedef(t, s)) => self.resolve_typedef(t, s, env, depth + 1),
            Some(Found::Param(Some(d), s)) => {
                // A type parameter: its default, which is what the
                // definition's own specialization uses.
                // Earlier parameters are visible in a default.
                self.resolve_env(d, s, env, depth + 1)
            }
            None => {
                let name = if path.len() == 2 && path[0] == "std" { path[1] } else { path[0] };
                match (path.len() == 1 || path[0] == "std", name) {
                    (true, "process") => TypeInfo { resolved: Resolved::Std("process"), ..other },
                    (true, "semaphore") => TypeInfo { resolved: Resolved::Std("semaphore"), ..other },
                    (true, "mailbox") => TypeInfo { resolved: Resolved::Std("mailbox"), ..other },
                    _ => other,
                }
            }
            _ => other,
        }
    }

    /// The type arguments in `#(...)` after a class name.
    fn bind_args(&self, c: usize, node: NodeRef, scope: usize, env: &Env) -> Env {
        let Some(pl) = self.child(node, ParamList) else {
            return Env::default();
        };
        let Some(al) = self.child(pl, ArgList) else {
            return Env::default();
        };
        let params = &self.classes[c].params;
        let mut map = HashMap::default();
        let mut pos = 0;
        for a in self.children(al) {
            if self.kind(a) != Arg {
                continue;
            }
            let own = self.own_tokens(a);
            let name = if own.first() == Some(&".") {
                own.get(1).map(|s| s.to_string())
            } else {
                pos += 1;
                params.get(pos - 1).map(|p| p.0.clone())
            };
            let Some(name) = name else { continue };
            let Some(value) = self.children(a).into_iter().next() else {
                continue;
            };
            if params.iter().any(|p| p.0 == name) {
                map.insert(name, Bound { node: value, scope, env: env.clone() });
            }
        }
        std::rc::Rc::new(map)
    }

    fn lookup_path(&self, path: &[&str], scope: usize, depth: usize) -> Option<Found> {
        // The first segment is a package or a class.
        let mut cur = if let Some(&p) = self.packages.get(path[0]) {
            p
        } else {
            match self.lookup(path[0], scope, depth)? {
                Found::Class(c) => self.classes[c].scope,
                // `typedef C #(T) t;` then `t::x`.
                Found::Typedef(t, s) => match self.resolve_typedef(t, s, &Env::default(), depth + 1).resolved {
                    Resolved::Class(c) => self.classes[c].scope,
                    _ => return None,
                },
                _ => return None,
            }
        };
        for (k, seg) in path[1..].iter().enumerate() {
            let last = k == path.len() - 2;
            let f = self.lookup_in(seg, cur, depth + 1)?;
            if last {
                return Some(f);
            }
            match f {
                Found::Class(c) => cur = self.classes[c].scope,
                _ => return None,
            }
        }
        None
    }

    fn resolve_typedef(&self, td: NodeRef, scope: usize, env: &Env, depth: usize) -> TypeInfo {
        let Some(dt) = self.child(td, DataType) else {
            return TypeInfo { resolved: Resolved::Other, shape: TypeShape::Other, args: Env::default() };
        };
        let mut info = self.resolve_env(dt, scope, env, depth + 1);
        // Unpacked dimensions after the typedef's name.
        if let Some(d) = self.children(td).into_iter().find(|c| self.kind(*c) == Dim) {
            info.shape = self.dim_shape(d, scope, depth);
        }
        info
    }

    /// The array kind of one unpacked dimension.
    fn dim_shape(&self, d: NodeRef, scope: usize, depth: usize) -> TypeShape {
        let toks: Vec<&str> = self.tokens(d).iter().map(|&t| self.text(d.tree, t)).collect();
        let inner = &toks[1..toks.len().saturating_sub(1)];
        match inner {
            [] => TypeShape::DynamicArray,
            ["$", ..] => TypeShape::Queue,
            ["*"] => TypeShape::AssociativeArray,
            [t] if sv_syntax::is_builtin_type(t) => TypeShape::AssociativeArray,
            _ => {
                // `[name]`: a type makes it associative, a value fixed.
                let first = inner[0];
                let is_type = inner.len() == 1
                    && self.tree(d).tokens[self.tokens(d)[1] as usize].kind == sv_syntax::TokenKind::Identifier
                    && matches!(
                        self.lookup(first, scope, depth + 1),
                        Some(Found::Class(_)) | Some(Found::Typedef(..)) | Some(Found::Param(..))
                    );
                if is_type {
                    TypeShape::AssociativeArray
                } else {
                    TypeShape::FixedUnpackedArray
                }
            }
        }
    }

    // ---- model --------------------------------------------------------------

    fn finish(mut self) -> Design {
        // Ids: plain classes first, then parameterized definitions, as in
        // slang's model.
        let mut order: Vec<usize> = (0..self.classes.len()).filter(|&c| !self.classes[c].generic).collect();
        order.extend((0..self.classes.len()).filter(|&c| self.classes[c].generic));
        let mut ids = vec![0 as ClassId; self.classes.len()];
        for (id, &c) in order.iter().enumerate() {
            ids[c] = id as ClassId;
        }
        let mut out = Vec::with_capacity(order.len());
        for (id, &c) in order.iter().enumerate() {
            out.push(self.make_class(c, id as ClassId, &ids));
        }
        // A definition nothing specialized takes its base by name.
        for i in 0..out.len() {
            if out[i].base.is_none() && !out[i].base_name.is_empty() {
                let name = out[i].base_name.clone();
                if let Some(b) = out.iter().position(|c| c.name == name) {
                    if b != i {
                        out[i].base = Some(b as ClassId);
                    }
                }
            }
        }
        // UVM roles follow the resolved base chain; a definition nothing
        // specialized has none.
        for (id, &c) in order.iter().enumerate() {
            if !self.classes[c].generic || self.used_names.contains(&self.classes[c].name) {
                let chain: Vec<&str> = self.chain(c).into_iter().map(|x| self.classes[x].name.as_str()).collect();
                out[id].uvm = classify(&chain);
            }
        }
        let files = std::mem::take(&mut self.locator).into_files();
        let counts = order.iter().map(|c| self.spec_counts.get(c).map_or(1, |&n| n as u32)).collect();
        let mut d = Design::from_parts(files, out, 0, Timings::default());
        d.set_specializations(counts);
        d
    }

    fn make_class(&mut self, c: usize, id: ClassId, ids: &[ClassId]) -> Class {
        let (r, generic, scope_idx, parent) =
            (self.classes[c].node, self.classes[c].generic, self.classes[c].scope, self.classes[c].parent);
        let name = self.classes[c].name.clone();
        let (_, name_tok) = self.name_of(r).unwrap_or_default();
        let loc = self.loc(r.tree, name_tok);
        let scope = match self.scopes[parent].kind {
            ScopeKind::Package | ScopeKind::Class(_) | ScopeKind::Design => self.scopes[parent].name.clone(),
            ScopeKind::Unit => String::new(),
        };
        let own = self.own_tokens(r);
        let is_interface = own.contains(&"interface");
        let is_virtual = own.contains(&"virtual");
        let specialized = !generic || self.used_names.contains(&name);
        let mut class = Class {
            id,
            name,
            scope,
            base_name: String::new(),
            base: None,
            uvm: UvmKind::None,
            is_virtual,
            is_parameterized: generic,
            is_library: false,
            is_interface,
            loc,
            properties: Vec::new(),
            methods: Vec::new(),
            constraints: Vec::new(),
            macros: self.macros_in(r),
            typedefs: Vec::new(),
        };
        if !specialized {
            // No specialization to read members from: the base as written
            // and the macros used, nothing else.
            class.is_virtual = false;
            class.is_interface = false;
            if let Some(dt) = self.classes[c].extends {
                let toks: Vec<&str> = self.tokens(dt).iter().map(|&t| self.text(dt.tree, t)).collect();
                let end = toks.iter().position(|t| *t == "#").unwrap_or(toks.len());
                class.base_name = toks[..end].last().copied().unwrap_or("").to_string();
            }
            return class;
        }
        let env = self.rep_env.get(&c).cloned().unwrap_or_default();
        if let Some((b, _)) = self.base_env(c, &env, 0) {
            class.base_name = self.classes[b].name.clone();
            class.base = Some(ids[b]);
        }
        for m in self.children(r) {
            match self.kind(m) {
                DataDecl => self.properties(m, scope_idx, &env, ids, &mut class.properties),
                CovergroupDecl => {
                    // An embedded covergroup is also a property of its type.
                    if let Some((n, t)) = self.name_of(m) {
                        let loc = self.loc(m.tree, t);
                        class.properties.push(Property {
                            name: n,
                            ty: "<unnamed covergroup>".into(),
                            type_class: None,
                            visibility: Visibility::Public,
                            is_static: false,
                            // The covergroup handle cannot be reassigned.
                            is_const: true,
                            is_rand: false,
                            is_virtual_interface: false,
                            loc,
                        });
                    }
                }
                FunctionDecl | TaskDecl => {
                    if let Some(method) = self.method(m, m, false, c, &env, ids) {
                        class.methods.push(method);
                    }
                }
                MethodPrototype => {
                    let own = self.own_tokens(m);
                    let pure = own.contains(&"pure");
                    let body =
                        self.name_of(m).and_then(|(n, _)| self.bodies.get(&(self.classes[c].name.clone(), n)).copied());
                    let body = if pure { Some(m) } else { body };
                    if let Some(b) = body {
                        if let Some(method) = self.method(m, b, true, c, &env, ids) {
                            class.methods.push(method);
                        }
                    }
                }
                ConstraintDecl | ConstraintPrototype => {
                    if let Some((n, t)) = self.name_of(m) {
                        let loc = self.loc(m.tree, t);
                        class.constraints.push(Constraint { name: n, loc });
                    }
                }
                TypedefDecl => {
                    if let Some((n, t)) = self.name_of(m) {
                        let loc = self.loc(m.tree, t);
                        class.typedefs.push(Typedef { name: n, loc });
                    }
                }
                _ => {}
            }
        }
        add_builtins(&mut class.methods, is_interface);
        class
    }

    /// Top-level macro uses that start inside the class's text (same file).
    fn macros_in(&mut self, r: NodeRef) -> Vec<MacroUse> {
        let t = self.tree(r);
        let Some((first, last)) = t.token_bounds(r.node) else {
            return Vec::new();
        };
        let (a, b) = (&t.tokens[first as usize], &t.tokens[last as usize]);
        // A class written by a macro has no text of its own to search.
        if a.from_macro || b.from_macro || a.origin.file != b.origin.file {
            return Vec::new();
        }
        let (file, start, end) = (a.origin.file, a.origin.offset, b.origin.offset + b.text.len() as u32);
        let uses: Vec<_> = t
            .macro_uses
            .iter()
            .filter(|u| u.origin.file == file && u.origin.offset >= start && u.origin.offset < end)
            .cloned()
            .collect();
        uses.into_iter()
            .map(|u| {
                let (f, line, column) = self.position(r.tree, u.origin);
                MacroUse { name: u.name.to_string(), loc: Loc { file: f, line, column, expansion: None } }
            })
            .collect()
    }

    fn class_id(&self, r: Resolved, ids: &[ClassId]) -> Option<ClassId> {
        match r {
            Resolved::Class(c) => Some(ids[c]),
            _ => None,
        }
    }

    fn properties(&mut self, d: NodeRef, scope: usize, env: &Env, ids: &[ClassId], out: &mut Vec<Property>) {
        let own = self.own_tokens(d);
        let visibility = visibility(&own);
        let (is_static, is_const) = (own.contains(&"static"), own.contains(&"const"));
        let is_rand = own.contains(&"rand") || own.contains(&"randc");
        let info = self.child(d, DataType).map(|dt| self.resolve_env(dt, scope, env, 0));
        let resolved = info.as_ref().map_or(Resolved::Other, |i| i.resolved);
        let array_type = info.as_ref().is_some_and(|i| i.shape != TypeShape::Other);
        for decl in self.children(d) {
            if self.kind(decl) != Declarator {
                continue;
            }
            let Some((name, t)) = self.name_of(decl) else {
                continue;
            };
            let loc = self.loc(decl.tree, t);
            // An array of class handles is not a class.
            let array = array_type || self.child(decl, Dim).is_some();
            let ty = self.type_string(self.child(d, DataType), Some(decl), scope, env, false);
            out.push(Property {
                name,
                ty,
                type_class: if array { None } else { self.class_id(resolved, ids) },
                visibility,
                is_static,
                is_const,
                is_rand,
                is_virtual_interface: resolved == Resolved::Vif,
                loc,
            });
        }
    }

    /// A method from its declaration `decl` (qualifiers) and `body`
    /// (name, ports, locals); the same node unless the body is out of block.
    fn method(
        &mut self,
        decl: NodeRef,
        body: NodeRef,
        is_extern: bool,
        c: usize,
        env: &Env,
        ids: &[ClassId],
    ) -> Option<Method> {
        let own = self.own_tokens(decl);
        let kw = own.iter().position(|t| *t == "function" || *t == "task")?;
        let is_task = own[kw] == "task";
        let (name, t) = self.name_of(body)?;
        let loc = self.loc(body.tree, t);
        let declared_virtual = own.contains(&"virtual");
        let is_pure = own.contains(&"pure");
        let is_static = own[..kw].contains(&"static");
        let scope = self.classes[c].scope;
        let returns_void = is_task
            || self
                .child(body, ReturnType)
                .is_some_and(|r| self.tokens(r).iter().map(|&t| self.text(r.tree, t)).collect::<Vec<_>>() == ["void"])
            || name == "new";
        let overrides = name != "new" && self.overrides_virtual(c, &name);
        let is_virtual = name != "new" && (declared_virtual || overrides);
        // An out-of-block body that overrides a virtual method is marked
        // virtual like its base (slang sets the flag when it joins them).
        let declared_virtual = declared_virtual || (is_extern && !is_pure && overrides);
        let phase = if FUNCTION_PHASES.contains(&name.as_str()) {
            PhaseKind::Function
        } else if TASK_PHASES.contains(&name.as_str()) {
            PhaseKind::Task
        } else {
            PhaseKind::None
        };
        let return_type = if is_task || name == "new" {
            "void".to_string()
        } else {
            let dt = self.child(body, ReturnType).and_then(|r| self.child(r, DataType));
            self.type_string(dt, None, scope, env, false)
        };
        let arguments = self.arguments(body, scope, env, ids);
        let locals = self.locals(body, scope, env);
        let facts = self.body_facts(decl, body, c, env, &name);
        Some(Method {
            name,
            visibility: visibility(&own),
            is_task,
            is_virtual,
            is_pure,
            is_static,
            phase,
            declared_virtual,
            is_builtin: false,
            is_extern,
            return_type,
            returns_void,
            loc,
            arguments,
            locals,
            calls: facts.calls,
            assignments: facts.assignments,
            timing_controls: facts.timing,
            calls_super_new: facts.calls_super_new || facts.super_new.is_some(),
            super_new: facts.super_new,
        })
    }

    /// A base class declares (or inherits) a virtual method of this name.
    fn overrides_virtual(&self, c: usize, name: &str) -> bool {
        // The nearest base that declares the name decides.
        let chain = self.chain_cached(c);
        for (k, &b) in chain.iter().enumerate().skip(1) {
            if let Some(&(first_virtual, _)) = self.method_decls(b).get(name) {
                if first_virtual {
                    return true;
                }
                // Not declared virtual: virtual if it overrides in turn.
                return chain[k + 1..].iter().any(|&x| self.method_decls(x).get(name).is_some_and(|d| d.1));
            }
        }
        false
    }

    /// [`Self::chain`], remembered per class (bindings are fixed by then).
    fn chain_cached(&self, c: usize) -> Rc<Vec<usize>> {
        if let Some(v) = self.chain_cache.borrow().get(&c) {
            return v.clone();
        }
        let v = Rc::new(self.chain(c));
        self.chain_cache.borrow_mut().insert(c, v.clone());
        v
    }

    /// Methods a class declares by name: whether the first declaration has
    /// `virtual`, and whether any does.
    fn method_decls(&self, c: usize) -> MethodDecls<'t> {
        if let Some(v) = self.decl_cache.borrow().get(&c) {
            return v.clone();
        }
        let mut map: HashMap<&'t str, (bool, bool)> = HashMap::default();
        for m in self.children(self.classes[c].node) {
            if !matches!(self.kind(m), FunctionDecl | TaskDecl | MethodPrototype) {
                continue;
            }
            let Some((n, _)) = self.name_str(m) else {
                continue;
            };
            let v = self.own_tokens(m).contains(&"virtual");
            let e = map.entry(n).or_insert((v, false));
            e.1 |= v;
        }
        let v = Rc::new(map);
        self.decl_cache.borrow_mut().insert(c, v.clone());
        v
    }

    fn arguments(&mut self, body: NodeRef, scope: usize, env: &Env, ids: &[ClassId]) -> Vec<Argument> {
        let mut out = Vec::new();
        let Some(pl) = self.child(body, PortList) else {
            return out;
        };
        let mut dir = Direction::In;
        let mut ty: Option<NodeRef> = None;
        for p in self.children(pl) {
            if self.kind(p) != Port {
                continue;
            }
            let own = self.own_tokens(p);
            let explicit_dir = own.iter().find_map(|t| match *t {
                "input" => Some(Direction::In),
                "output" => Some(Direction::Out),
                "inout" => Some(Direction::InOut),
                "ref" => Some(Direction::Ref),
                _ => None,
            });
            let dt = self.child(p, DataType);
            match (explicit_dir, dt) {
                (_, Some(d)) => ty = Some(d),
                // A direction without a type means logic.
                (Some(_), None) => ty = None,
                // Neither: both carry over from the previous port.
                (None, None) => {}
            }
            if let Some(d) = explicit_dir {
                dir = d;
            }
            let Some(decl) = self.child(p, Declarator) else {
                continue;
            };
            let Some((name, t)) = self.name_of(decl) else {
                continue;
            };
            let loc = self.loc(decl.tree, t);
            let info = ty.map(|d| self.resolve_env(d, scope, env, 0));
            let mut shape = info.as_ref().map_or(TypeShape::Other, |i| i.shape);
            if let Some(d) = self.children(decl).into_iter().find(|c| self.kind(*c) == Dim) {
                shape = self.dim_shape(d, scope, 0);
            }
            let type_class =
                if shape == TypeShape::Other { info.and_then(|i| self.class_id(i.resolved, ids)) } else { None };
            out.push(Argument {
                name,
                ty: self.type_string(ty, Some(decl), scope, env, false),
                canonical_type: self.type_string(ty, Some(decl), scope, env, true),
                type_class,
                direction: dir,
                shape,
                loc,
            });
        }
        out
    }

    fn locals(&mut self, body: NodeRef, scope: usize, env: &Env) -> Vec<Local> {
        let mut out = Vec::new();
        let Some(b) = self.child(body, Body) else {
            return out;
        };
        for d in self.children(b) {
            if self.kind(d) != DataDecl {
                continue;
            }
            let vif =
                self.child(d, DataType).is_some_and(|dt| self.resolve_env(dt, scope, env, 0).resolved == Resolved::Vif);
            for decl in self.children(d) {
                if self.kind(decl) != Declarator {
                    continue;
                }
                if let Some((name, t)) = self.name_of(decl) {
                    let loc = self.loc(decl.tree, t);
                    let ty = self.type_string(self.child(d, DataType), Some(decl), scope, env, false);
                    out.push(Local { name, ty, is_virtual_interface: vif, loc });
                }
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Found {
    Class(usize),
    Typedef(NodeRef, usize),
    /// A type parameter with its default type, and the class scope.
    Param(Option<NodeRef>, usize),
}

fn visibility(own: &[&str]) -> Visibility {
    if own.contains(&"local") {
        Visibility::Local
    } else if own.contains(&"protected") {
        Visibility::Protected
    } else {
        Visibility::Public
    }
}

const FUNCTION_PHASES: &[&str] = &[
    "build_phase",
    "connect_phase",
    "end_of_elaboration_phase",
    "start_of_simulation_phase",
    "extract_phase",
    "check_phase",
    "report_phase",
    "final_phase",
];

const TASK_PHASES: &[&str] = &[
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

/// The UVM role from the names along the base chain, most specific first
/// (as slang's model does it).
fn classify(chain: &[&str]) -> UvmKind {
    let has = |n: &str| chain.contains(&n);
    if has("uvm_component") {
        for (base, kind) in [
            ("uvm_test", UvmKind::Test),
            ("uvm_env", UvmKind::Env),
            ("uvm_agent", UvmKind::Agent),
            ("uvm_driver", UvmKind::Driver),
            ("uvm_monitor", UvmKind::Monitor),
            ("uvm_sequencer_base", UvmKind::Sequencer),
            ("uvm_scoreboard", UvmKind::Scoreboard),
            ("uvm_subscriber", UvmKind::Subscriber),
        ] {
            if has(base) {
                return kind;
            }
        }
        return UvmKind::Component;
    }
    if has("uvm_sequence_base") || has("uvm_sequence") {
        return UvmKind::Sequence;
    }
    if has("uvm_sequence_item") {
        return UvmKind::SequenceItem;
    }
    if has("uvm_object") {
        UvmKind::Object
    } else {
        UvmKind::None
    }
}

/// The methods every class has (randomize and friends), after its own
/// members, unless the class declares one itself.
/// Built-in class methods (IEEE 1800 18.5-18.13). Interface classes get
/// all but rand_mode and constraint_mode, as in Slang.
fn add_builtins(methods: &mut Vec<Method>, is_interface: bool) {
    let arg_ty = |a: &str| match a {
        "seed" => "int",
        "state" => "string",
        _ => "bit",
    };
    let builtin = |name: &str, virt: bool, void: bool, arg: Option<&str>| Method {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_task: false,
        is_virtual: virt,
        is_pure: false,
        is_static: false,
        phase: PhaseKind::None,
        declared_virtual: virt,
        is_builtin: true,
        is_extern: false,
        return_type: match name {
            "randomize" => "int",
            "get_randstate" => "string",
            _ => "void",
        }
        .to_string(),
        returns_void: void,
        loc: Loc::default(),
        arguments: arg
            .map(|a| Argument {
                name: a.to_string(),
                ty: arg_ty(a).to_string(),
                canonical_type: arg_ty(a).to_string(),
                type_class: None,
                direction: Direction::In,
                shape: TypeShape::Other,
                loc: Loc::default(),
            })
            .into_iter()
            .collect(),
        locals: Vec::new(),
        calls: Vec::new(),
        assignments: Vec::new(),
        timing_controls: Vec::new(),
        calls_super_new: false,
        super_new: None,
    };
    for (name, virt, void, arg) in [
        ("randomize", true, false, None),
        ("pre_randomize", false, true, None),
        ("post_randomize", false, true, None),
        ("get_randstate", false, false, None),
        ("set_randstate", false, true, Some("state")),
        ("srandom", false, true, Some("seed")),
        ("rand_mode", false, true, Some("on_ff")),
        ("constraint_mode", false, true, Some("on_ff")),
    ] {
        if is_interface && name.ends_with("_mode") {
            continue;
        }
        if !methods.iter().any(|m| m.name == name) {
            methods.push(builtin(name, virt, void, arg));
        }
    }
}
