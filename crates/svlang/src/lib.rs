//! Pure-Rust IEEE 1800-2023 SystemVerilog compiler front end and RTL netlist SDK.
//!
//! `svlang` gives Rust tools such as linters, language servers, model checkers,
//! equivalence checkers, documentation generators, and design-exploration utilities
//! a single dependency covering the SystemVerilog front-end and RTL lowering pipeline
//! with zero C++ dependencies and zero UVM coupling:
//!
//! - [`source`]: VCS and Xcelium `-f`/`-F` filelist reader with `$VAR` expansion
//! - [`lexer`]: lossless IEEE 1800-2023 tokenizer
//! - [`preprocessor`]: Clause 22 macro, `` `include ``, and `` `ifdef `` engine with token provenance
//! - [`syntax`]: error-recovering lossless Concrete Syntax Tree (CST)
//! - [`semantic`]: scopes, 4-state arbitrary-width constant evaluation, elaborated
//!   design hierarchy, bound expressions and SVA properties, compiler checks, and OOP class model
//! - [`netlist`]: format-neutral word-level RTL netlist IR, symbolic procedural execution,
//!   bit-level driver analysis, SVA lowering, and memory blasting
//!
//! Configure inputs on a [`Driver`] and build a [`Compilation`]:
//!
//! ```
//! use svlang::{Compilation, SyntaxKind};
//!
//! let comp = Compilation::from_source(
//!     "counter.sv",
//!     "module counter #(parameter int W = 8) (input logic clk, output logic [W-1:0] q);\n\
//!        always_ff @(posedge clk) q <= q + 1'b1;\n\
//!      endmodule\n",
//! );
//! assert!(comp.diagnostics().is_empty());
//! assert_eq!(comp.elaborate().hierarchy, ["$root instance counter : counter", "counter parameter W = 8"]);
//! assert_eq!(
//!     comp.trees()[0].preorder().into_iter().filter(|&n| comp.trees()[0].kind(n) == SyntaxKind::ProceduralBlock).count(),
//!     1
//! );
//! ```

use std::fmt;
use std::path::PathBuf;

pub use sv_lexer as lexer;
pub use sv_netlist as netlist;
pub use sv_preprocessor as preprocessor;
pub use sv_semantic as semantic;
pub use sv_source as source;
pub use sv_syntax as syntax;

pub use sv_netlist::{LowerConfig, LowerOutcome, Netlist, NetlistDiag};

pub use sv_semantic::{
    BinaryOp, BitState, BoundExpr, BoundProperty, CVal, CycleBound, CycleRange, Design, ElabItem, ElabItemKind,
    Elaborated, ElaboratedDesign, Error, ExprArena, ExprBinder, ExprDiag, ExprId, ExprKind, InsideItem, Instance,
    InstanceId, Int, Loc, LocalBinding, LocalId, Locator, PackageScope, PackedField, PortConn, PortDir, PropExpr,
    PropKind, RepeatKind, RtlType, SampledKind, ScopeNode, ScopeSignal, SemaChecks, SeqExpr, SvaBinder, SvaClock,
    SvaEdge, Symbol, SymbolId, SymbolKind, SysFunc, UnaryOp,
};
pub use sv_source::{Dialect, FileList, Severity};
pub use sv_syntax::{NodeId, SyntaxKind, TokenKind, Tree};

/// Pipeline stage that produced a [`Diagnostic`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticStage {
    /// Filelist option or include/library path resolution.
    FileList,
    /// Clause 22 preprocessing (`define, `include, `ifdef).
    Preprocessor,
    /// Syntax / grammar parsing.
    Parser,
    /// Name resolution, typing, class rules, SVA/UDP rules, or elaboration.
    Semantic,
}

/// A resolved compiler diagnostic with 1-based source coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// File path (or virtual buffer name).
    pub file: String,
    /// 1-based line number (0 when unknown).
    pub line: u32,
    /// 1-based byte column (0 when reported at a macro expansion site).
    pub column: u32,
    /// Front-end stage that produced the diagnostic.
    pub stage: DiagnosticStage,
    /// Severity level (`Error` or `Warning`).
    pub severity: Severity,
    /// Human-readable error or warning text.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{}:{}:{}: {kind}: {}", self.file, self.line, self.column, self.message)
    }
}

#[derive(Clone, Debug)]
enum InputUnit {
    File(PathBuf),
    Memory { name: String, text: String },
}

/// Configures source files, in-memory buffers, defines, include paths, and
/// simulator filelists (`-f` / `-F`), then parses or compiles them into a
/// [`Compilation`].
#[derive(Clone, Debug)]
pub struct Driver {
    dialect: Dialect,
    single_unit: bool,
    include_dirs: Vec<PathBuf>,
    defines: Vec<(String, Option<String>)>,
    tops: Vec<String>,
    params: Vec<(String, String)>,
    inputs: Vec<InputUnit>,
    file_list: Option<FileList>,
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver {
    /// Creates an empty [`Driver`] accepting [`Dialect::Any`] options.
    pub fn new() -> Self {
        Self {
            dialect: Dialect::Any,
            single_unit: false,
            include_dirs: Vec::new(),
            defines: Vec::new(),
            tops: Vec::new(),
            params: Vec::new(),
            inputs: Vec::new(),
            file_list: None,
        }
    }

    /// Restricts CLI / filelist option parsing to `dialect`.
    pub fn dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Compiles all file inputs as a single compilation unit (`$unit` shared),
    /// matching `slang --single-unit`.
    pub fn single_unit(mut self, enabled: bool) -> Self {
        self.single_unit = enabled;
        self
    }

    /// Appends an `` `include `` search directory.
    pub fn include_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.include_dirs.push(dir.into());
        self
    }

    /// Defines a preprocessor macro (`+define+NAME` or `+define+NAME=VALUE`).
    pub fn define(mut self, name: impl Into<String>, value: Option<String>) -> Self {
        self.defines.push((name.into(), value));
        self
    }

    /// Adds an explicit top-level module name (`-top`) for elaboration.
    pub fn top(mut self, module: impl Into<String>) -> Self {
        self.tops.push(module.into());
        self
    }

    /// Adds a top-level or hierarchical parameter override (`-G name=value`).
    pub fn param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push((name.into(), value.into()));
        self
    }

    /// Adds a SystemVerilog source file from disk.
    pub fn add_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.inputs.push(InputUnit::File(path.into()));
        self
    }

    /// Adds an in-memory SystemVerilog source buffer named `name`.
    pub fn add_source(mut self, name: impl Into<String>, text: impl Into<String>) -> Self {
        self.inputs.push(InputUnit::Memory { name: name.into(), text: text.into() });
        self
    }

    /// Reads simulator CLI arguments (`-f`, `-F`, `+incdir+`, `+define+`, `-I`,
    /// `-D`, `-y`, `-v`, `-top`, `-G`, `--single-unit`, and source paths) relative to
    /// the current working directory.
    pub fn add_args<I, S>(&mut self, args: I) -> Result<&FileList, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let raw: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
        if raw.iter().any(|a| a == "--single-unit") {
            self.single_unit = true;
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let list = FileList::read_args(
            raw.iter().map(String::as_str).filter(|&a| a != "--single-unit"),
            &cwd,
            self.dialect,
            &source::ProcessEnv,
        );
        if let Some(o) = list.ignored.iter().find(|o| o.value.starts_with("--")) {
            return Err(Error(format!("unknown command line argument '{}'", o.value)));
        }
        let errs: Vec<&str> = list
            .diagnostics
            .iter()
            .filter(|d| {
                if d.severity != source::Severity::Error {
                    return false;
                }
                if let Some(p) = d.message.strip_prefix("source file not found: ") {
                    if syntax::overlay::get(std::path::Path::new(p)).is_some() {
                        return false;
                    }
                }
                true
            })
            .map(|d| d.message.as_str())
            .collect();
        if !errs.is_empty() {
            return Err(Error(errs.join("; ")));
        }
        for d in &list.include_dirs {
            self.include_dirs.push(d.value.clone());
        }
        for (n, v) in list.effective_defines() {
            self.defines.push((n, v));
        }
        for t in &list.top {
            self.tops.push(t.value.clone());
        }
        for p in &list.param_overrides {
            self.params.push(p.value.clone());
        }
        for s in &list.sources {
            self.inputs.push(InputUnit::File(s.value.clone()));
        }
        self.file_list = Some(list);
        Ok(self.file_list.as_ref().expect("just assigned"))
    }

    /// Returns the parsed [`FileList`] if [`Self::add_args`] was called.
    pub fn file_list(&self) -> Option<&FileList> {
        self.file_list.as_ref()
    }

    fn syntax_options(&self) -> syntax::Options {
        syntax::Options {
            include_dirs: self.include_dirs.clone(),
            defines: self
                .defines
                .iter()
                .map(|(n, v)| match v {
                    Some(v) => format!("{n}={v}"),
                    None => n.clone(),
                })
                .collect(),
            ..syntax::Options::default()
        }
    }

    /// Preprocesses and parses all configured inputs into lossless CST [`Tree`]s,
    /// resolving `-v` / `-y` libraries when configured via [`Self::add_args`].
    pub fn parse(&self) -> Result<Vec<Tree>, Error> {
        if self.inputs.is_empty() {
            return Err(Error("no sources given".into()));
        }
        let opts = self.syntax_options();
        let all_files = self.inputs.iter().all(|u| matches!(u, InputUnit::File(_)));
        let mut trees = if all_files {
            let paths: Vec<PathBuf> = self
                .inputs
                .iter()
                .filter_map(|u| match u {
                    InputUnit::File(p) => Some(p.clone()),
                    InputUnit::Memory { .. } => None,
                })
                .collect();
            for f in &paths {
                if syntax::overlay::get(f).is_some() {
                    continue;
                }
                let regular = std::fs::metadata(f).is_ok_and(|m| m.is_file());
                if !regular || std::fs::File::open(f).is_err() {
                    return Err(Error(format!("could not read the sources: {}", f.display())));
                }
            }
            if self.single_unit {
                vec![syntax::parse_unit(&paths, opts.clone())]
            } else {
                syntax::parse_files(&paths, opts.clone())
            }
        } else {
            let mut pp = preprocessor::Preprocessor::new(opts.clone());
            let mut out = Vec::with_capacity(self.inputs.len());
            for u in &self.inputs {
                match u {
                    InputUnit::File(f) => {
                        if syntax::overlay::get(f).is_none() {
                            let regular = std::fs::metadata(f).is_ok_and(|m| m.is_file());
                            if !regular || std::fs::File::open(f).is_err() {
                                return Err(Error(format!("could not read the sources: {}", f.display())));
                            }
                        }
                        out.push(syntax::parse_output(pp.run_file(f)));
                    }
                    InputUnit::Memory { name, text } => {
                        out.push(syntax::parse_output(pp.run_source(text, name)));
                    }
                }
            }
            out
        };
        if let Some(list) = &self.file_list {
            semantic::library::add_library_trees(&mut trees, list, &opts);
        }
        Ok(trees)
    }

    /// Parses all inputs and wraps them in a [`Compilation`] ready for semantic
    /// analysis, elaboration, RTL lowering, and custom lint passes.
    pub fn compile(self) -> Result<Compilation, Error> {
        let trees = self.parse()?;
        let file_list_diags = self
            .file_list
            .iter()
            .flat_map(|fl| &fl.diagnostics)
            .map(|d| Diagnostic {
                file: d.origin.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                line: d.origin.line,
                column: 0,
                stage: DiagnosticStage::FileList,
                severity: d.severity,
                message: d.message.clone(),
            })
            .collect();
        Ok(Compilation { trees, tops: self.tops, params: self.params, file_list_diags })
    }
}

/// A parsed SystemVerilog compilation ready for semantic queries, elaboration,
/// RTL netlist lowering, type inspection, and custom AST/CST analysis.
pub struct Compilation {
    trees: Vec<Tree>,
    tops: Vec<String>,
    params: Vec<(String, String)>,
    file_list_diags: Vec<Diagnostic>,
}

impl Compilation {
    /// Creates a [`Compilation`] from parsed CST [`Tree`]s.
    pub fn from_trees(trees: Vec<Tree>) -> Self {
        Self { trees, tops: Vec::new(), params: Vec::new(), file_list_diags: Vec::new() }
    }

    /// Creates a [`Compilation`] from parsed [`Tree`]s and explicit `-top` module names.
    pub fn from_trees_with_tops(trees: Vec<Tree>, tops: Vec<String>) -> Self {
        Self { trees, tops, params: Vec::new(), file_list_diags: Vec::new() }
    }

    /// Attaches `-G name=value` parameter overrides to this compilation.
    pub fn with_params(mut self, params: Vec<(String, String)>) -> Self {
        self.params = params;
        self
    }

    /// Convenience constructor that parses one in-memory SystemVerilog source string.
    pub fn from_source(name: &str, src: &str) -> Self {
        let tree = syntax::parse_source(src, name, syntax::Options::default());
        Self { trees: vec![tree], tops: Vec::new(), params: Vec::new(), file_list_diags: Vec::new() }
    }

    /// Compiles command-line / filelist arguments directly into a [`Compilation`].
    pub fn from_args<I, S>(args: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut driver = Driver::new();
        driver.add_args(args)?;
        driver.compile()
    }

    /// Parsed lossless CSTs in compilation order (including resolved library trees).
    pub fn trees(&self) -> &[Tree] {
        &self.trees
    }

    /// Consumes the compilation and returns its CST [`Tree`]s.
    pub fn into_trees(self) -> Vec<Tree> {
        self.trees
    }

    /// Explicit top module names (`-top`), or empty when inferred automatically.
    pub fn tops(&self) -> &[String] {
        &self.tops
    }

    /// Configured `-G name=value` parameter overrides.
    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    /// Returns `true` if any tree recorded preprocessor or syntax errors.
    pub fn has_syntax_errors(&self) -> bool {
        self.trees.iter().any(|t| !t.errors.is_empty() || !t.pp_diagnostics.is_empty())
    }

    /// Builds a fresh source [`Locator`] for mapping `(tree_idx, token_idx)` to
    /// 1-based `(file, line, column)`.
    pub fn locator(&self) -> Locator {
        Locator::default()
    }

    /// Builds lexical and design-unit [`semantic::scopes::Scopes`] across all trees.
    pub fn scopes(&self) -> semantic::scopes::Scopes {
        semantic::scopes::build(&self.trees)
    }

    /// Builds scopes together with resolved symbol type strings and constant parameter values.
    pub fn symbol_info(&self) -> (semantic::scopes::Scopes, Vec<Option<String>>, Vec<Option<String>>) {
        let scopes = self.scopes();
        let (types, values) = semantic::symbol_info(&self.trees, &scopes);
        (scopes, types, values)
    }

    /// Builds an elaborated [`semantic::oracle::TypeOracle`] for querying declared
    /// types and constant values at arbitrary CST nodes.
    ///
    /// Note that `TypeOracle` elaborates from [`Self::tops`] using source-level parameter
    /// values without applying CLI `-G` [`Self::params`] overrides. For per-instance RTL
    /// typing with `-G` parameter overrides, use [`Self::elaborate`] and [`ExprBinder`].
    pub fn type_oracle(&self) -> semantic::oracle::TypeOracle<'_> {
        let mut oracle = semantic::oracle::TypeOracle::new(&self.trees);
        oracle.elaborate(&self.tops);
        oracle
    }

    /// Elaborates the module/interface instance hierarchy and parameter values.
    pub fn elaborate(&self) -> Elaborated {
        semantic::elaborate_with_params(&self.trees, &self.tops, &self.params)
    }

    /// Builds the owned IEEE 1800 OOP class and file [`Design`] model.
    pub fn design(&self) -> Design {
        semantic::design_of(&self.trees, 0.0)
    }

    /// Runs preprocessor, syntax, semantic, and elaboration checks, returning
    /// both the resolved [`Diagnostic`] list and the [`Elaborated`] hierarchy.
    pub fn check(&self) -> (Vec<Diagnostic>, Elaborated) {
        let mut loc = Locator::default();
        let mut out = self.file_list_diags.clone();
        for (ix, t) in self.trees.iter().enumerate() {
            collect_syntax_diagnostics(ix, t, &mut loc, &mut out);
        }
        let (sema, elab) = semantic::semantic_diags_with_params(&self.trees, &self.tops, &self.params);
        for d in sema {
            let t = &self.trees[d.tree];
            let Some((tok, _)) = t.token_bounds(d.node) else {
                continue;
            };
            let l = loc.loc(d.tree, t, tok);
            let (file, line, column) = match l.expansion {
                Some((f, line)) => (f, line, 0),
                None => (l.file, l.line, l.column),
            };
            let file = loc.files().get(file as usize).cloned().unwrap_or_default();
            out.push(Diagnostic {
                file,
                line,
                column,
                stage: DiagnosticStage::Semantic,
                severity: Severity::Error,
                message: d.message,
            });
        }
        (out, elab)
    }

    /// Returns all filelist, preprocessor, parser, semantic, and elaboration diagnostics.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.check().0
    }

    /// Elaborates and lowers this compilation into a format-neutral, word-level RTL [`Netlist`]
    /// and bound SVA [`BoundProperty`] trees, returning a fail-closed [`LowerOutcome`].
    ///
    /// # Examples
    ///
    /// ```
    /// use svlang::{Compilation, LowerConfig, LowerOutcome};
    ///
    /// let comp = Compilation::from_source(
    ///     "reg_en.sv",
    ///     "module reg_en (input logic clk, en, input logic [3:0] d, output logic [3:0] q);\n\
    ///        always_ff @(posedge clk) if (en) q <= d;\n\
    ///      endmodule\n",
    /// );
    /// let nl = match comp.lower_netlist(&LowerConfig::default()) {
    ///     LowerOutcome::Lowered(nl) => nl,
    ///     other => panic!("unexpected outcome: {other:?}"),
    /// };
    /// assert_eq!(nl.top, "reg_en");
    /// assert_eq!(nl.states.len(), 1);
    /// assert_eq!(nl.states[0].width, 4);
    /// ```
    pub fn lower_netlist(&self, config: &netlist::LowerConfig) -> netlist::LowerOutcome {
        netlist::lower_trees(&self.trees, &self.tops, &self.params, config)
    }

    /// Parses and binds an external SVA property or concurrent assertion string
    /// directly inside the elaborated scope `instance_path` (`"top"`, `"top.u0"`, or
    /// `"top.u0.g[0]"`), without modifying or splicing into source files.
    ///
    /// Both bare property specifications (`"@(posedge clk) req |-> ##[1:3] ack"`, `"q < MAX"`)
    /// and full concurrent directives (`"assume property (@(posedge clk) en)"`,
    /// `"lbl: cover property (done);"`) are accepted.
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<Diagnostic>)` when `instance_path` does not name an elaborated
    /// instance or generate scope, when `prop_src` is empty or fails to parse, or when
    /// expression or SVA binding fails closed (reporting `<property>` with 1-based line
    /// and column coordinates relative to `prop_src`).
    ///
    /// # Examples
    ///
    /// ```
    /// use svlang::{Compilation, ExprArena, LowerConfig, PropKind};
    ///
    /// let comp = Compilation::from_source(
    ///     "cnt.sv",
    ///     "module cnt (input logic clk, rst, output logic [3:0] q);\n\
    ///        always_ff @(posedge clk) if (rst) q <= 4'd0; else q <= q + 4'd1;\n\
    ///      endmodule\n",
    /// );
    /// let elab = comp.elaborate();
    /// let mut nl = comp.lower_netlist(&LowerConfig::default()).unwrap_netlist();
    /// let mut arena = ExprArena::new();
    /// let prop = comp
    ///     .bind_property_in_scope(&elab.design, "cnt", "@(posedge clk) disable iff (rst) q <= 4'd15", &mut arena)
    ///     .unwrap();
    /// assert_eq!(prop.kind, PropKind::Assert);
    /// let lowered = nl.lower_external_property(&elab.design, &arena, &prop, false).unwrap();
    /// assert!(!lowered.is_empty());
    /// ```
    pub fn bind_property_in_scope(
        &self,
        design: &ElaboratedDesign,
        instance_path: &str,
        prop_src: &str,
        arena: &mut ExprArena,
    ) -> Result<BoundProperty, Vec<Diagnostic>> {
        let Some((inst_id, scope_prefix)) = semantic::sva_ir::resolve_scope_path(design, instance_path) else {
            return Err(vec![Diagnostic {
                file: "<property>".into(),
                line: 1,
                column: 1,
                stage: DiagnosticStage::Semantic,
                severity: Severity::Error,
                message: format!("unknown elaborated scope '{instance_path}'"),
            }]);
        };
        let body_no_semi = prop_src.trim_end().trim_end_matches(';').trim_end();
        let trimmed_start = body_no_semi.trim_start();
        if trimmed_start.is_empty() {
            return Err(vec![Diagnostic {
                file: "<property>".into(),
                line: 1,
                column: 1,
                stage: DiagnosticStage::Parser,
                severity: Severity::Error,
                message: "empty property string".into(),
            }]);
        }
        let is_full_directive = starts_with_concurrent_directive(trimmed_start);
        let prefix =
            if is_full_directive { "module __svlang_cli_sva; " } else { "module __svlang_cli_sva; assert property (" };
        let suffix = if is_full_directive { "\n;\nendmodule\n" } else { "\n);\nendmodule\n" };
        let prefix_cols = prefix.len() as u32;
        let user_lines = (body_no_semi.bytes().filter(|&b| b == b'\n').count() + 1) as u32;
        let last_line_end_col = (body_no_semi.rsplit('\n').next().map_or(0, str::len) as u32) + 1;
        let adjust_tmp_pos = |line: u32, col: u32| -> (u32, u32) {
            if line == 1 {
                (1, col.saturating_sub(prefix_cols).max(1))
            } else if line > user_lines {
                (user_lines, last_line_end_col)
            } else {
                (line, col)
            }
        };

        let wrapped = format!("{prefix}{body_no_semi}{suffix}");
        const TMP_PROP_FILE: &str = "\0<property>";
        let tmp_tree = syntax::parse_source(&wrapped, TMP_PROP_FILE, syntax::Options::default());
        let mut parse_diags = Vec::new();
        let mut loc = Locator::default();
        collect_syntax_diagnostics(0, &tmp_tree, &mut loc, &mut parse_diags);
        if !parse_diags.is_empty() {
            for d in &mut parse_diags {
                d.file = "<property>".into();
                if d.line >= 1 {
                    let (l, c) = adjust_tmp_pos(d.line, d.column);
                    d.line = l;
                    d.column = c;
                }
            }
            return Err(parse_diags);
        }
        let Some(ca_node) = tmp_tree.preorder().into_iter().find(|&n| tmp_tree.kind(n) == SyntaxKind::ConcurrentAssert)
        else {
            return Err(vec![Diagnostic {
                file: "<property>".into(),
                line: 1,
                column: 1,
                stage: DiagnosticStage::Parser,
                severity: Severity::Error,
                message: "invalid SVA property syntax".into(),
            }]);
        };

        // Gather active `genvar` constants visible in `scope_prefix`, innermost scope first.
        let inst = design.instance(inst_id);
        let mut scoped_genvars: Vec<(usize, String, CVal)> = Vec::new();
        for &sid in &inst.symbols {
            let sym = design.symbol(sid);
            if sym.kind == SymbolKind::Genvar {
                if let Some(cv) = &sym.cval {
                    if sym.rel_path == sym.name
                        || scope_prefix == sym.init_scope_prefix
                        || scope_prefix.starts_with(&format!("{}.", sym.init_scope_prefix))
                    {
                        scoped_genvars.push((sym.init_scope_prefix.len(), sym.name.clone(), cv.clone()));
                    }
                }
            }
        }
        scoped_genvars.sort_by_key(|a| std::cmp::Reverse(a.0));
        let gen_consts: Vec<(String, CVal)> = scoped_genvars.into_iter().map(|(_, name, cv)| (name, cv)).collect();

        let arena_start = arena.exprs.len();
        let tmp_idx = self.trees.len();
        let mut binder = SvaBinder::new(&self.trees, design, arena).with_extra_tree(&tmp_tree);
        let bound = binder.bind_concurrent_assert(tmp_idx, ca_node, inst_id, &scope_prefix, &gen_consts, None);
        if !binder.diags.is_empty() {
            let errs = binder
                .diags
                .into_iter()
                .map(|d| {
                    let from_tmp = d.loc.file != 0 && (d.loc.file as usize) >= design.files.len();
                    let (file, line, column) = if from_tmp {
                        let (l, c) = adjust_tmp_pos(d.loc.line, d.loc.column);
                        ("<property>".to_string(), l, c)
                    } else if d.loc.file == 0 {
                        ("<property>".to_string(), d.loc.line, d.loc.column)
                    } else {
                        let f = design
                            .files
                            .get(d.loc.file as usize)
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| "<property>".into());
                        (f, d.loc.line, d.loc.column)
                    };
                    Diagnostic {
                        file,
                        line,
                        column,
                        stage: DiagnosticStage::Semantic,
                        severity: Severity::Error,
                        message: d.message,
                    }
                })
                .collect();
            return Err(errs);
        }
        let Some(mut bp) = bound else {
            return Err(vec![Diagnostic {
                file: "<property>".into(),
                line: 1,
                column: 1,
                stage: DiagnosticStage::Semantic,
                severity: Severity::Error,
                message: "failed to bind SVA property".into(),
            }]);
        };
        if bp.loc.file != 0 && (bp.loc.file as usize) >= design.files.len() {
            let (l, c) = adjust_tmp_pos(bp.loc.line, bp.loc.column);
            bp.loc.line = l;
            bp.loc.column = c;
        }
        for expr in &mut arena.exprs[arena_start..] {
            if expr.loc.file != 0 && (expr.loc.file as usize) >= design.files.len() {
                let (l, c) = adjust_tmp_pos(expr.loc.line, expr.loc.column);
                expr.loc.line = l;
                expr.loc.column = c;
            }
        }
        Ok(bp)
    }
}

fn starts_with_concurrent_directive(s: &str) -> bool {
    let body = match s.split_once(':') {
        Some((lbl, rest))
            if !rest.starts_with('=') && lbl.trim().chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
        {
            rest.trim_start()
        }
        _ => s,
    };
    let kw: String = body.chars().take_while(|c| c.is_ascii_alphabetic() || *c == '_').collect();
    matches!(kw.as_str(), "assert" | "assume" | "cover" | "restrict" | "expect")
}

fn collect_syntax_diagnostics(ix: usize, t: &Tree, loc: &mut Locator, out: &mut Vec<Diagnostic>) {
    let file_of = |loc: &Locator, f: u32| loc.files().get(f as usize).cloned().unwrap_or_default();
    for d in &t.pp_diagnostics {
        let (f, line, column) = loc.position(ix, t, d.origin);
        out.push(Diagnostic {
            file: file_of(loc, f),
            line,
            column,
            stage: DiagnosticStage::Preprocessor,
            severity: Severity::Error,
            message: d.message.clone(),
        });
    }
    for e in &t.errors {
        let tok = (e.token as usize).min(t.tokens.len().saturating_sub(1)) as u32;
        if t.tokens.is_empty() {
            out.push(Diagnostic {
                file: String::new(),
                line: 0,
                column: 0,
                stage: DiagnosticStage::Parser,
                severity: Severity::Error,
                message: e.message.clone(),
            });
            continue;
        }
        let l = loc.loc(ix, t, tok);
        let (file, line, column) = match l.expansion {
            Some((f, line)) => (f, line, 0),
            None => (l.file, l.line, l.column),
        };
        out.push(Diagnostic {
            file: file_of(loc, file),
            line,
            column,
            stage: DiagnosticStage::Parser,
            severity: Severity::Error,
            message: e.message.clone(),
        });
    }
}
