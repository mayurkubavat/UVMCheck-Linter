//! Caches shared by the threads that run the rules. Each value is built
//! once per key: the first thread to ask builds it, the others wait for it.
//!
//! Keys hold a generation that [`lint_all`](crate::lint_all) bumps, so that
//! a later compile whose trees reuse the same addresses never sees an
//! earlier compile's values.

use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

type Value = Arc<dyn Any + Send + Sync>;
type Key = (&'static str, usize, usize, usize);

/// A thread-local cache of one value per compile, keyed by
/// [`Cst::compile_key`](crate::syntax::Cst::compile_key).
pub(crate) type CompileCache<T> = std::cell::RefCell<Option<((usize, usize), std::rc::Rc<T>)>>;

/// [`Key`] with the name as its address, plus the generation.
type LocalSlot = (usize, usize, usize, usize, usize);

static SLOTS: Mutex<Option<HashMap<Key, Arc<OnceLock<Value>>>>> = Mutex::new(None);
static NEXT_GEN: AtomicUsize = AtomicUsize::new(1);

thread_local! {
    static GEN: Cell<usize> = const { Cell::new(0) };
    /// This thread's copies of the values it got, by (name, key, generation).
    static LOCAL: std::cell::RefCell<svlang::semantic::fx::FxMap<LocalSlot, Value>> = Default::default();
}

/// The settings of the runs under way, by generation: a run's threads take
/// theirs from here when they [`enter`].
static SETTINGS: Mutex<Vec<(usize, Arc<crate::session::Settings>)>> = Mutex::new(Vec::new());

/// Starts a generation for one lint run under `settings` and makes it this
/// thread's.
pub(crate) fn begin(settings: Arc<crate::session::Settings>) -> usize {
    let g = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).push((g, settings.clone()));
    GEN.with(|c| c.set(g));
    crate::session::bind(Some(settings));
    g
}

/// Makes generation `g` this thread's (a worker of that run), with the
/// run's settings.
pub(crate) fn enter(g: usize) {
    GEN.with(|c| c.set(g));
    let settings =
        SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| *k == g).map(|(_, s)| s.clone());
    crate::session::bind(settings);
}

/// This thread's generation, for threads it starts to [`enter`].
pub(crate) fn current() -> usize {
    GEN.with(Cell::get)
}

/// Drops the values of generation `g` and returns this thread to none.
pub(crate) fn end(g: usize) {
    if let Ok(mut m) = SLOTS.lock() {
        if let Some(m) = m.as_mut() {
            m.retain(|k, _| k.3 != g);
        }
    }
    LOCAL.with(|l| l.borrow_mut().retain(|k, _| k.4 != g));
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(k, _)| *k != g);
    GEN.with(|c| c.set(0));
    crate::session::bind(None);
}

/// The value `name` for `key`, built by `make` the first time.
pub(crate) fn get<T: Any + Send + Sync>(name: &'static str, key: (usize, usize), make: impl FnOnce() -> T) -> Arc<T> {
    let g = GEN.with(Cell::get);
    // Values this thread already has, without the lock (rules ask often).
    let local = (name.as_ptr() as usize, name.len(), key.0, key.1, g);
    if let Some(v) = LOCAL.with(|l| l.borrow().get(&local).cloned()) {
        if let Ok(v) = v.downcast::<T>() {
            return v;
        }
    }
    let slot = {
        let mut m = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        m.get_or_insert_with(HashMap::new).entry((name, key.0, key.1, g)).or_default().clone()
    };
    let v = slot
        .get_or_init(|| {
            static PROFILE: OnceLock<bool> = OnceLock::new();
            if !*PROFILE.get_or_init(|| std::env::var_os("DV_LINT_PROFILE").is_some()) {
                return Arc::new(make()) as Value;
            }
            let t = std::time::Instant::now();
            let v = Arc::new(make()) as Value;
            eprintln!("pshared {name} {:.1}", t.elapsed().as_secs_f64() * 1e3);
            v
        })
        .clone();
    LOCAL.with(|l| l.borrow_mut().insert(local, v.clone()));
    v.downcast::<T>().unwrap_or_else(|_| panic!("shared cache {name}: wrong type"))
}

/// `src` lexed with its trivia, once per source text (several rules and
/// shared builds read the same files).
pub(crate) fn raw_lex(src: &str) -> Arc<Vec<svlang::lexer::Token>> {
    // Keyed by the text's address: only within a lint run, whose end drops it.
    if current() == 0 {
        return Arc::new(svlang::lexer::lex(src));
    }
    get("raw_lex", (src.as_ptr() as usize, src.len()), || svlang::lexer::lex(src))
}

/// sv-semantic's semantic checks of the compile.
pub(crate) fn sema_checks(c: &crate::syntax::Cst) -> Arc<svlang::semantic::SemaChecks> {
    get("sema_checks", c.compile_key(), || svlang::semantic::SemaChecks::new(c.all))
}

/// The compile's elaboration (from the `-top` modules).
pub(crate) fn elaboration(c: &crate::syntax::Cst) -> Arc<svlang::semantic::Elaborated> {
    get("elaboration", c.compile_key(), || svlang::semantic::elaborate_full(c.all, &crate::tops()))
}
