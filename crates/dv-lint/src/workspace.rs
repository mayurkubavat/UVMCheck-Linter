//! A project as an editor or a host program sees it: its configuration
//! loaded once, and checked again whenever the sources change.
//!
//! ```no_run
//! let ws = dv_lint::Workspace::open(std::path::Path::new("."))?;
//! let checked = ws.check(&[])?;
//! for f in &checked.findings {
//!     let (file, line, col) = f.position_in(&checked.files);
//!     println!("{file}:{line}:{col}: {} {}", f.rule, f.message);
//! }
//! # Ok::<(), String>(())
//! ```
//!
//! The findings are the ones the command line reports for the project:
//! the same rule selection, configuration, waivers and baseline. Text set
//! with [`svlang::syntax::overlay`] stands in for files on disk, so a language
//! server checks what is in the editor.

use crate::{project, suppress, Finding, Session};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A project folder with its configuration, waivers and baseline.
pub struct Workspace {
    root: PathBuf,
    config: Option<project::Config>,
    suppress: suppress::Suppress,
    session: Session,
    keep: HashSet<&'static str>,
    /// What loading the configuration and the waivers warned about.
    pub warnings: Vec<String>,
}

/// The result of [`Workspace::check`].
pub struct Checked {
    /// The file table the findings' locations index.
    pub files: Vec<String>,
    /// What is left after the configuration, waivers and baseline.
    pub findings: Vec<Finding>,
    /// How many findings waivers and the baseline took out.
    pub waived: usize,
}

/// The project config in `root` or a folder above it.
fn find_config(root: &Path) -> Option<PathBuf> {
    // `find` looks from the folder of a source: any name in `root` does.
    let probe = root.join("x.sv");
    let abs = if probe.is_relative() { std::env::current_dir().ok()?.join(probe) } else { probe };
    let mut dir = abs.parent();
    while let Some(d) = dir {
        if let Some(f) = project::NAMES.iter().map(|n| d.join(n)).find(|f| f.is_file()) {
            return Some(f);
        }
        dir = d.parent();
    }
    None
}

/// `.svh`, `.vh` and the like: text meant to be included, not compiled.
fn is_header(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e, "svh" | "vh" | "h" | "inc" | "svi"))
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl Workspace {
    /// Opens the project of folder `root`: the config found there or above
    /// (none is fine), the waiver file and the baseline next to it.
    pub fn open(root: &Path) -> Result<Workspace, String> {
        let mut warnings = Vec::new();
        let config = match find_config(root) {
            Some(file) => {
                let cfg = project::Config::load(&file)?;
                warnings.extend(cfg.warnings.iter().map(|w| format!("{}: {w}", file.display())));
                Some(cfg)
            }
            None => None,
        };
        let root = config
            .as_ref()
            .map(|c| c.root.clone())
            .filter(|r| !r.as_os_str().is_empty())
            .unwrap_or_else(|| root.to_path_buf());
        let suppress = suppress::Suppress::load_in(&root, config.as_ref(), None, None, false)?;
        warnings.extend(suppress.warnings.iter().cloned());
        let mut session = Session::new();
        if let Some(c) = &config {
            session.set_options(&c.options).set_style(&c.style).set_naming(&c.naming);
        }
        // The command line's selection: the project's preset, else the
        // default profiles, plus the rules the config configures.
        let selected = crate::rule_selection(config.as_ref().and_then(|c| c.preset.as_deref()), false)?;
        let keep = crate::rules()
            .into_iter()
            .filter(|r| selected(r.id) || config.as_ref().is_some_and(|c| c.selects(r.id, r.name)))
            .map(|r| r.id)
            .collect();
        Ok(Workspace { root, config, suppress, session, keep, warnings })
    }

    /// The project's folder (the config's, when there is one).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The config file in use.
    pub fn config_file(&self) -> Option<&Path> {
        self.config.as_ref().map(|c| c.file.as_path())
    }

    /// Lints the project's sources, and each of `open` the project does not
    /// compile as a file of its own (every one of them when the config names
    /// no sources). Headers nothing compiled are left out: alone they do
    /// not parse.
    pub fn check(&self, open: &[PathBuf]) -> Result<Checked, String> {
        let args: &[String] = self.config.as_ref().map_or(&[], |c| &c.compile_args);
        let cwd = std::env::current_dir().unwrap_or_default();
        let list =
            svlang::source::FileList::read_args(args, &cwd, svlang::source::Dialect::Any, &svlang::source::ProcessEnv);
        // Include folders and defines, for files parsed on their own.
        let mut shared: Vec<String> =
            list.include_dirs.iter().map(|d| format!("+incdir+{}", d.value.display())).collect();
        shared.extend(list.effective_defines().into_iter().map(|(n, v)| match v {
            Some(v) => format!("+define+{n}={v}"),
            None => format!("+define+{n}"),
        }));
        let mut trees = Vec::new();
        if !list.sources.is_empty() {
            trees = svlang::semantic::parse_args(args).map_err(|e| e.to_string())?.0;
        }
        let compiled: HashSet<PathBuf> = trees.iter().flat_map(|t| t.files.iter().map(|f| canonical(f))).collect();
        let mut seen = HashSet::new();
        let alone: Vec<String> = open
            .iter()
            .filter(|p| !is_header(p) && !compiled.contains(&canonical(p)) && seen.insert(canonical(p)))
            .map(|p| p.display().to_string())
            .collect();
        if !alone.is_empty() {
            shared.extend(alone);
            trees.extend(svlang::semantic::parse_args(&shared).map_err(|e| e.to_string())?.0);
        }
        if trees.is_empty() {
            return Ok(Checked { files: Vec::new(), findings: Vec::new(), waived: 0 });
        }
        let design = dv_model::design_of(&trees, 0.0);
        let waived_before = crate::inline_waived();
        let (files, mut findings) = self.session.lint_all(&design, &trees, |id| self.keep.contains(id));
        let inline = crate::inline_waived() - waived_before;
        crate::fix::guard_macro_calls(&mut findings);
        // The editor shows the user's code: never the UVM library's findings.
        let sources: Vec<std::path::PathBuf> = trees.iter().filter_map(|t| t.files.first().cloned()).collect();
        crate::library::Library::detect(&sources).hide(
            &mut findings,
            &files,
            self.config.as_ref().is_some_and(|c| c.reports_library_macros()),
        );
        if let Some(c) = &self.config {
            c.apply(&mut findings, &files);
        }
        let waived = inline + self.suppress.apply(&mut findings, &files);
        crate::expansions::name_macros(&mut findings, &files);
        Ok(Checked { files, findings, waived })
    }
}
