//! UVM-FAC-003 (UvmFactoryMissingUtils): Concrete classes derived from
//! uvm_object or uvm_component must declare factory registration macros
//! (`uvm_component_utils / `uvm_object_utils).

use crate::support::uvm_kinds::{is_component, is_object};
use crate::{find_method, find_property, Severity, Sink};
use dv_model::Design;

pub(crate) fn uvm_factory_missing_utils(d: &Design, csts: &[crate::syntax::Cst], out: &mut Sink) {
    let ix = crate::support::class_roles::Index::new(csts);
    const REGISTERS: [&str; 4] =
        ["uvm_component_utils", "uvm_object_utils", "uvm_component_param_utils", "uvm_object_param_utils"];
    for cls in d.classes() {
        if cls.is_parameterized || cls.is_virtual || cls.is_interface || cls.name.starts_with("uvm_") {
            continue;
        }
        let (comp, obj) = (is_component(d, cls), is_object(d, cls));
        if !comp && !obj {
            continue;
        }
        // A member of its own named type_id or get_type, or a registration macro.
        let declares = |n: &str| {
            cls.typedefs.iter().any(|t| t.name == n) || find_property(cls, n).is_some() || find_method(cls, n).is_some()
        };
        if declares("type_id") || declares("get_type") {
            continue;
        }
        if cls.macros.iter().any(|m| REGISTERS.iter().any(|r| m.name.starts_with(r))) {
            continue;
        }
        let (kind, mac) =
            if comp { ("uvm_component", "`uvm_component_utils") } else { ("uvm_object", "`uvm_object_utils") };
        out.add(
            "UVM-FAC-003",
            "UvmFactoryMissingUtils",
            Severity::Warning,
            format!(
                "Concrete class '{0}' derived from '{kind}' is not registered with the UVM factory. Add '{mac}({0})' to enable testbench factory overrides.",
                cls.name
            ),
            cls.loc,
        )
        .fixes
        .extend(ix.decl(cls.loc).and_then(|(c, n)| register_fix(c, csts, n, mac, &cls.name)));
    }
}

/// Inserts the registration macro after the class header, at the indent of
/// its first item. Parameterized classes need the *_param_utils form with
/// their parameters, so they get no fix; nor does a class the macro is not
/// defined for. Unsafe: get_type_name() and create() change, and
/// `uvm_object_utils needs a constructor whose name argument has a default.
fn register_fix(
    c: &crate::syntax::Cst,
    csts: &[crate::syntax::Cst],
    cls: svlang::syntax::NodeId,
    mac: &str,
    name: &str,
) -> Option<crate::fix::Fix> {
    use svlang::syntax::SyntaxKind::*;
    if c.kind(cls) != ClassDecl {
        return None;
    }
    let header = c.tree.child(cls, Header)?;
    if c.tree.child(header, ParamList).is_some() {
        return None;
    }
    let defined = |m: &str| {
        csts.iter().any(|k| {
            k.tree.sources.iter().any(|s| {
                let pat = format!("`define {}", &m[1..]);
                s.match_indices(&pat).any(|(i, _)| {
                    s.as_bytes().get(i + pat.len()).is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                })
            })
        })
    };
    if !defined(mac) {
        return None;
    }
    let semi = c.toks(header).filter(|&t| c.text_of(t) == ";").last()?;
    let st = &c.tree.tokens[semi as usize];
    if st.from_macro {
        return None;
    }
    let src = c.tree.sources.get(st.origin.file as usize)?.as_bytes();
    let first_item =
        c.tree.children(cls).filter(|&k| !matches!(c.kind(k), Name | Header | EndLabel)).find_map(|k| c.first_tok(k));
    let indent = match first_item {
        Some(t) if !c.tree.tokens[t as usize].from_macro => {
            let pos = c.tree.tokens[t as usize].origin.offset as usize;
            let mut start = pos;
            while start > 0 && src[start - 1] != b'\n' {
                start -= 1;
            }
            let mut end = start;
            while end < pos && matches!(src[end], b' ' | b'\t') {
                end += 1;
            }
            String::from_utf8_lossy(&src[start..end]).into_owned()
        }
        _ => "    ".to_string(),
    };
    let at = st.origin.offset as usize + 1;
    let mut f = c.file_fix(st.origin.file, at, at, format!("\n{indent}{mac}({name})"), "Register with the factory");
    f.safety = crate::fix::FixSafety::Unsafe;
    Some(f)
}
