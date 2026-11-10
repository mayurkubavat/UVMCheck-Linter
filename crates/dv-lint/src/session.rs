//! A lint session: the settings one run of the rules reads, held in a value
//! instead of in the process.
//!
//! ```no_run
//! # let (design, trees): (dv_model::Design, Vec<svlang::syntax::Tree>) = todo!();
//! let mut session = dv_lint::Session::new();
//! session.add_tops(&["tb_top".to_string()]).set_skip_libraries(true);
//! let (files, findings) = session.lint_all(&design, &trees, |_| true);
//! ```
//!
//! Two sessions with different settings can run at the same time in one
//! process (an editor with two workspaces, a test harness, a host
//! language): each run hands its settings to the threads it starts.
//!
//! The free functions ([`crate::options::set`], [`crate::add_tops`],
//! [`crate::lint_all`], ...) work on the process's own session, as the
//! command line does: one project per process.

use crate::options::{NamingOverride, StyleValue};
use crate::Finding;
use dv_model::Design;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// What the rules of one run read besides the sources.
#[derive(Clone, Debug, Default)]
pub(crate) struct Settings {
    /// `rules.<RULE>.options`: (rule ID, key) -> value.
    pub(crate) options: HashMap<(String, String), i64>,
    /// The `style` block.
    pub(crate) style: HashMap<String, StyleValue>,
    /// The `naming` block, by target.
    pub(crate) naming: HashMap<String, NamingOverride>,
    /// `-top`: elaboration starts from these only.
    pub(crate) tops: Vec<String>,
    /// `--skip-libraries`: report nothing in `-v`/`-y` files.
    pub(crate) skip_libraries: bool,
}

/// The process's session: what the free setter functions change.
static PROCESS: RwLock<Option<Arc<Settings>>> = RwLock::new(None);

thread_local! {
    /// The settings of the run this thread works for.
    static CURRENT: RefCell<Option<Arc<Settings>>> = const { RefCell::new(None) };
}

/// The process session's settings as they are now.
pub(crate) fn process() -> Arc<Settings> {
    PROCESS.read().ok().and_then(|p| p.clone()).unwrap_or_default()
}

/// Changes the process session's settings.
pub(crate) fn change_process(f: impl FnOnce(&mut Settings)) {
    if let Ok(mut p) = PROCESS.write() {
        f(Arc::make_mut(p.get_or_insert_with(Default::default)));
    }
}

/// Makes `settings` this thread's (a thread of that run), or none.
pub(crate) fn bind(settings: Option<Arc<Settings>>) {
    CURRENT.with(|c| *c.borrow_mut() = settings);
}

/// Reads the settings in force on this thread: its run's, else the
/// process's (outside a run: the command line's reports and commands).
pub(crate) fn with<R>(f: impl FnOnce(&Settings) -> R) -> R {
    CURRENT.with(|c| match c.borrow().as_deref() {
        Some(s) => f(s),
        None => f(&process()),
    })
}

/// The settings of one or more lint runs.
#[derive(Clone, Debug, Default)]
pub struct Session {
    settings: Settings,
}

impl Session {
    /// A session with every setting at its default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Rule options: (rule ID or name, key, value).
    pub fn set_options(&mut self, options: &[(String, String, i64)]) -> &mut Self {
        self.settings.options = crate::options::resolved(options);
        self
    }

    /// The `style` block: (key, value).
    pub fn set_style(&mut self, style: &[(String, StyleValue)]) -> &mut Self {
        self.settings.style = style.iter().cloned().collect();
        self
    }

    /// The `naming` block: (target, settings).
    pub fn set_naming(&mut self, naming: &[(String, NamingOverride)]) -> &mut Self {
        self.settings.naming = naming.iter().cloned().collect();
        self
    }

    /// Adds top modules (`-top`): elaboration starts from them only.
    pub fn add_tops(&mut self, tops: &[String]) -> &mut Self {
        self.settings.tops.extend(tops.iter().cloned());
        self
    }

    /// Reports nothing in library files (`-v`, `-y`).
    pub fn set_skip_libraries(&mut self, on: bool) -> &mut Self {
        self.settings.skip_libraries = on;
        self
    }

    /// [`crate::lint`] under this session's settings.
    pub fn lint(&self, design: &Design, keep: impl Fn(&str) -> bool) -> Vec<Finding> {
        self.lint_all(design, &[], keep).1
    }

    /// [`crate::lint_all`] under this session's settings.
    pub fn lint_all(
        &self,
        design: &Design,
        trees: &[svlang::syntax::Tree],
        keep: impl Fn(&str) -> bool,
    ) -> (Vec<String>, Vec<Finding>) {
        crate::lint_from(Arc::new(self.settings.clone()), crate::Source::Ready(design), trees, keep).1
    }

    /// [`crate::lint_built`] under this session's settings.
    pub fn lint_built(
        &self,
        build: impl FnOnce() -> Design,
        trees: &[svlang::syntax::Tree],
        keep: impl Fn(&str) -> bool,
    ) -> (Design, (Vec<String>, Vec<Finding>)) {
        let settings = Arc::new(self.settings.clone());
        let (d, out) = crate::lint_from(settings, crate::Source::Build(Box::new(build)), trees, keep);
        (d.expect("a built design"), out)
    }
}
