//! Rule options from the project configuration (`rules.<RULE>.options`),
//! as the C++ rules read them with `swe::ruleOption`: an integer per rule
//! and key, the rule's own default when the config does not set it.
//!
//! The values live in the run's [`Session`](crate::Session); the setters
//! here change the process's own session.

use crate::session;
use std::collections::HashMap;

/// Options by (rule ID, key), with rule names resolved to IDs.
pub(crate) fn resolved(options: &[(String, String, i64)]) -> HashMap<(String, String), i64> {
    let rules = crate::rules();
    let id_of =
        |k: &str| -> String { rules.iter().find(|r| r.name == k).map_or_else(|| k.to_string(), |r| r.id.to_string()) };
    options.iter().map(|(r, k, v)| ((id_of(r), k.clone()), *v)).collect()
}

/// Sets the options: (rule ID or name, key, value). Names are resolved to
/// IDs here.
pub fn set(options: &[(String, String, i64)]) {
    let m = resolved(options);
    session::change_process(|s| s.options = m);
}

/// The value of option `key` of rule `id`, else `default`.
pub(crate) fn get(id: &str, key: &str, default: i64) -> i64 {
    session::with(|s| s.options.get(&(id.to_string(), key.to_string())).copied()).unwrap_or(default)
}

/// A `style` setting from the project configuration.
#[derive(Clone, Debug)]
pub enum StyleValue {
    Bool(bool),
    Int(i64),
    Str(String),
}

/// Sets the `style` block (key, value), as uvmcheck's StyleConfiguration.
pub fn set_style(style: &[(String, StyleValue)]) {
    session::change_process(|s| s.style = style.iter().cloned().collect());
}

fn style(key: &str) -> Option<StyleValue> {
    session::with(|s| s.style.get(key).cloned())
}

pub(crate) fn style_bool(key: &str, default: bool) -> bool {
    match style(key) {
        Some(StyleValue::Bool(b)) => b,
        _ => default,
    }
}

pub(crate) fn style_int(key: &str, default: i64) -> i64 {
    match style(key) {
        Some(StyleValue::Int(n)) => n,
        _ => default,
    }
}

pub(crate) fn style_str(key: &str, default: &str) -> String {
    match style(key) {
        Some(StyleValue::Str(s)) => s,
        _ => default.to_string(),
    }
}

/// One `naming.<Target>` block: the keys it sets.
#[derive(Clone, Debug, Default)]
pub struct NamingOverride {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub pattern: Option<String>,
    pub casing: Option<String>,
    pub severity: Option<String>,
    pub enabled: Option<bool>,
}

/// Sets the `naming` block (target, settings).
pub fn set_naming(naming: &[(String, NamingOverride)]) {
    session::change_process(|s| s.naming = naming.iter().cloned().collect());
}

pub(crate) fn naming(target: &str) -> Option<NamingOverride> {
    session::with(|s| s.naming.get(target).cloned())
}
