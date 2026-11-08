//! Rule options from the project configuration (`rules.<RULE>.options`),
//! as the C++ rules read them with `swe::ruleOption`: an integer per rule
//! and key, the rule's own default when the config does not set it.

use std::collections::HashMap;
use std::sync::RwLock;

static OPTIONS: RwLock<Option<HashMap<(String, String), i64>>> = RwLock::new(None);

/// Sets the options: (rule ID or name, key, value). Names are resolved to
/// IDs here.
pub fn set(options: &[(String, String, i64)]) {
    let rules = crate::rules();
    let id_of =
        |k: &str| -> String { rules.iter().find(|r| r.name == k).map_or_else(|| k.to_string(), |r| r.id.to_string()) };
    let m = options.iter().map(|(r, k, v)| ((id_of(r), k.clone()), *v)).collect();
    if let Ok(mut o) = OPTIONS.write() {
        *o = Some(m);
    }
}

/// The value of option `key` of rule `id`, else `default`.
pub(crate) fn get(id: &str, key: &str, default: i64) -> i64 {
    OPTIONS
        .read()
        .ok()
        .and_then(|o| o.as_ref().and_then(|m| m.get(&(id.to_string(), key.to_string())).copied()))
        .unwrap_or(default)
}
