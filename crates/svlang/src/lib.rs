//! Pure-Rust IEEE 1800-2023 SystemVerilog compiler front end.
//!
//! `svlang` gives Rust tools — custom linters, language servers, documentation
//! generators, code formatters, and design-exploration utilities — a single
//! dependency covering the entire SystemVerilog front-end pipeline with zero C++
//! dependencies and zero UVM coupling:
//!
//! - [`source`] — VCS and Xcelium `-f`/`-F` filelist reader with `$VAR` expansion
//! - [`lexer`] — lossless IEEE 1800-2023 tokenizer
//! - [`preprocessor`] — Clause 22 macro/include/`ifdef` engine with token provenance
//! - [`syntax`] — error-recovering lossless Concrete Syntax Tree (CST)
//! - [`semantic`] — scopes, 4-state arbitrary-width constant evaluation, expression
//!   type oracle, module hierarchy elaboration, compiler checks, and OOP class model
//!
//! For high-level workflows (similar to C++ `slang`), configure inputs on a
//! [`Driver`] and build a [`Compilation`]:
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
pub use sv_preprocessor as preprocessor;
pub use sv_semantic as semantic;
pub use sv_source as source;
pub use sv_syntax as syntax;

pub use sv_semantic::{Design, Elaborated, Error, Loc, Locator, SemaChecks};
pub use sv_source::{Dialect, FileList};
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
    /// Human-readable error or warning text.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}: error: {}", self.file, self.line, self.column, self.message)
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
    /// `-D`, `-y`, `-v`, `-top`, `--single-unit`, and source paths) relative to
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
        if let Some(d) = list.diagnostics.iter().find(|d| {
            d.severity == source::Severity::Error
                && (d.message.starts_with("cannot read file list")
                    || d.message.starts_with("file list includes itself")
                    || d.message.starts_with("library file not found")
                    || d.message.starts_with("library directory not found"))
        }) {
            return Err(Error(d.message.clone()));
        }
        if let Some(o) = list.ignored.iter().find(|o| o.value.starts_with("--")) {
            return Err(Error(format!("unknown command line argument '{}'", o.value)));
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
    /// analysis, elaboration, and custom lint passes.
    pub fn compile(self) -> Result<Compilation, Error> {
        let trees = self.parse()?;
        Ok(Compilation { trees, tops: self.tops })
    }
}

/// A parsed SystemVerilog compilation ready for semantic queries, elaboration,
/// type inspection, and custom AST/CST analysis.
pub struct Compilation {
    trees: Vec<Tree>,
    tops: Vec<String>,
}

impl Compilation {
    /// Creates a [`Compilation`] from parsed CST [`Tree`]s.
    pub fn from_trees(trees: Vec<Tree>) -> Self {
        Self { trees, tops: Vec::new() }
    }

    /// Creates a [`Compilation`] from parsed [`Tree`]s and explicit `-top` module names.
    pub fn from_trees_with_tops(trees: Vec<Tree>, tops: Vec<String>) -> Self {
        Self { trees, tops }
    }

    /// Convenience constructor that parses one in-memory SystemVerilog source string.
    pub fn from_source(name: &str, src: &str) -> Self {
        let tree = syntax::parse_source(src, name, syntax::Options::default());
        Self { trees: vec![tree], tops: Vec::new() }
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
    pub fn type_oracle(&self) -> semantic::oracle::TypeOracle<'_> {
        let mut oracle = semantic::oracle::TypeOracle::new(&self.trees);
        oracle.elaborate(&self.tops);
        oracle
    }

    /// Elaborates the module/interface instance hierarchy and parameter values.
    pub fn elaborate(&self) -> Elaborated {
        semantic::elaborate_full(&self.trees, &self.tops)
    }

    /// Builds the owned IEEE 1800 OOP class and file [`Design`] model.
    pub fn design(&self) -> Design {
        semantic::design_of(&self.trees, 0.0)
    }

    /// Runs preprocessor, syntax, semantic, and elaboration checks, returning
    /// both the resolved [`Diagnostic`] list and the [`Elaborated`] hierarchy.
    pub fn check(&self) -> (Vec<Diagnostic>, Elaborated) {
        let mut loc = Locator::default();
        let mut out = Vec::new();
        for (ix, t) in self.trees.iter().enumerate() {
            collect_syntax_diagnostics(ix, t, &mut loc, &mut out);
        }
        let (sema, elab) = semantic::semantic_diags(&self.trees, &self.tops);
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
            out.push(Diagnostic { file, line, column, stage: DiagnosticStage::Semantic, message: d.message });
        }
        (out, elab)
    }

    /// Returns all preprocessor, parser, semantic, and elaboration diagnostics.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.check().0
    }
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
            message: e.message.clone(),
        });
    }
}
