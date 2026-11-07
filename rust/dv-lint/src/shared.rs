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

static SLOTS: Mutex<Option<HashMap<Key, Arc<OnceLock<Value>>>>> = Mutex::new(None);
static NEXT_GEN: AtomicUsize = AtomicUsize::new(1);

thread_local! {
    static GEN: Cell<usize> = const { Cell::new(0) };
}

/// Starts a generation for one lint run and makes it this thread's.
pub(crate) fn begin() -> usize {
    let g = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
    GEN.with(|c| c.set(g));
    g
}

/// Makes generation `g` this thread's (a worker of that run).
pub(crate) fn enter(g: usize) {
    GEN.with(|c| c.set(g));
}

/// Drops the values of generation `g` and returns this thread to none.
pub(crate) fn end(g: usize) {
    if let Ok(mut m) = SLOTS.lock() {
        if let Some(m) = m.as_mut() {
            m.retain(|k, _| k.3 != g);
        }
    }
    GEN.with(|c| c.set(0));
}

/// The value `name` for `key`, built by `make` the first time.
pub(crate) fn get<T: Any + Send + Sync>(name: &'static str, key: (usize, usize), make: impl FnOnce() -> T) -> Arc<T> {
    let g = GEN.with(Cell::get);
    let slot = {
        let mut m = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        m.get_or_insert_with(HashMap::new).entry((name, key.0, key.1, g)).or_default().clone()
    };
    let v = slot.get_or_init(|| Arc::new(make()) as Value).clone();
    v.downcast::<T>().unwrap_or_else(|_| panic!("shared cache {name}: wrong type"))
}

/// sv-sema's semantic checks of the compile.
pub(crate) fn sema_checks(c: &crate::syntax::Cst) -> Arc<sv_sema::SemaChecks> {
    get("sema_checks", c.compile_key(), || sv_sema::SemaChecks::new(c.all))
}

/// The compile's elaboration (from the `-top` modules).
pub(crate) fn elaboration(c: &crate::syntax::Cst) -> Arc<sv_sema::Elaborated> {
    get("elaboration", c.compile_key(), || sv_sema::elaborate_full(c.all, &crate::tops()))
}
