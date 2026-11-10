//! UVM-DAG-001 (UvmDanglingConfigString): Static UVM component DAG path
//! matching: detects glob strings in uvm_config_db::set() that match zero
//! instantiated components in the verification hierarchy.

use crate::support::uvm_classes::{args, call_name, calls_in, is_component, string_value};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

/// `globMatch` of UvmModel: `*` and `?` wildcards.
fn glob_match(p: &[u8], t: &[u8]) -> bool {
    let (mut pi, mut ti) = (0, 0);
    let (mut star_p, mut star_t) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_p = Some(pi);
            pi += 1;
            star_t = ti;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

pub(crate) fn dangling_config_string(d: &Design, csts: &[Cst], s: &mut Sink) {
    let mut paths = Vec::new();
    for cls in d.classes().iter().filter(|c| !c.is_parameterized) {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = p.type_class else { continue };
            if is_component(d, t) {
                paths.push(p.name.clone());
                paths.push(format!("uvm_test_top.{}", p.name));
                paths.push(format!("uvm_test_top.m_env.{}", p.name));
            }
        }
    }
    if paths.is_empty() {
        return;
    }
    let matches = |pat: &str| {
        pat.is_empty()
            || pat == "*"
            || paths.iter().any(|p| glob_match(pat.as_bytes(), p.as_bytes()) || p.contains(pat))
    };
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            if c.kind(n) != CallExpr {
                continue;
            }
            let name = call_name(c, n);
            let text = c.text(n);
            let left = c.tree.children(n).next().map(|l| c.text(l)).unwrap_or_default();
            let set = (name == "set" && left.contains("uvm_config_db"))
                || (text.contains("uvm_config_db") && text.contains("set"));
            if !set {
                continue;
            }
            let a = args(c, n);
            if a.len() < 4 {
                continue;
            }
            let Some(inst) = a[1] else { continue };
            let Some(pat) = string_value(c, inst) else { continue };
            if pat.is_empty() || pat == "*" || matches(&pat) {
                continue;
            }
            let Some(t) = c.first_tok(inst) else { continue };
            s.add(
                "UVM-DAG-001",
                "UvmDanglingConfigString",
                Severity::Error,
                format!(
                    "Dangling UVM config_db path: string pattern '{pat}' matches 0 instantiated components in the static verification hierarchy DAG. \
                     Typographical errors in config_db target strings cause configurations to be silently dropped."
                ),
                c.loc(t),
            );
        }
    }
}
