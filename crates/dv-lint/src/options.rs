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

/// A `style` setting from the project configuration.
#[derive(Clone, Debug)]
pub enum StyleValue {
    Bool(bool),
    Int(i64),
    Str(String),
}

static STYLE: RwLock<Option<HashMap<String, StyleValue>>> = RwLock::new(None);

/// Sets the `style` block (key, value), as uvmcheck's StyleConfiguration.
pub fn set_style(style: &[(String, StyleValue)]) {
    if let Ok(mut s) = STYLE.write() {
        *s = Some(style.iter().cloned().collect());
    }
}

fn style(key: &str) -> Option<StyleValue> {
    STYLE.read().ok().and_then(|s| s.as_ref().and_then(|m| m.get(key).cloned()))
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

static NAMING: RwLock<Option<HashMap<String, NamingOverride>>> = RwLock::new(None);

/// Sets the `naming` block (target, settings).
pub fn set_naming(naming: &[(String, NamingOverride)]) {
    if let Ok(mut n) = NAMING.write() {
        *n = Some(naming.iter().cloned().collect());
    }
}

pub(crate) fn naming(target: &str) -> Option<NamingOverride> {
    NAMING.read().ok().and_then(|n| n.as_ref().and_then(|m| m.get(target).cloned()))
}
