//! SV-DED-015 (NeverSetClassParameter): Flags a parameter of a user
//! parameterized class that no specialization in the linted design ever sets.

use crate::support::class_index::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeMap;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// A class's port parameters (not localparams): name and name token.
fn port_params(c: &Cst, cls: NodeId) -> Vec<(String, u32)> {
    let Some(h) = c.tree.child(cls, Header) else {
        return Vec::new();
    };
    let Some(pl) = c.tree.child(h, ParamList) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in c.tree.children(pl).filter(|&x| c.kind(x) == ParamDecl) {
        if c.toks(p).any(|t| c.text_of(t) == "localparam") {
            continue;
        }
        // `int W = 8` has a Declarator; `type T = int` names it directly.
        let names: Vec<NodeId> = c
            .tree
            .children(p)
            .filter_map(|x| match c.kind(x) {
                Declarator => c.tree.child(x, Name),
                Name => Some(x),
                _ => None,
            })
            .collect();
        for nm in names {
            if let Some(t) = c.first_tok(nm) {
                out.push((c.text_of(t).to_string(), t));
            }
        }
    }
    out
}

pub(crate) fn never_set_parameter(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    // Generic user classes some non-generic user class uses as a base or a
    // property type.
    let mut generics: Vec<usize> = Vec::new();
    // The built-in generic `mailbox #(type T)`, used as a property type.
    let mut std_mailbox = false;
    for i in classes.user() {
        let cls = &classes.list[i];
        if cls.generic {
            continue;
        }
        let mut note = |name: &str| {
            if let Some(k) = classes.find(name) {
                if classes.list[k].generic && !classes.list[k].library && !generics.contains(&k) {
                    generics.push(k);
                }
            }
        };
        if let Some(b) = cls.base.as_deref() {
            note(b);
        }
        for p in &cls.props {
            if let Some(n) = p.ty.and_then(|t| type_name(c, t)) {
                if n == "mailbox" && classes.find(&n).is_none() {
                    std_mailbox = true;
                }
                note(&n);
            }
        }
    }
    // slang reports the std class's parameter with no location.
    if std_mailbox
        && !c.nodes(DataType).into_iter().any(|ty| {
            type_name(c, ty).as_deref() == Some("mailbox")
                && c.tree.child(ty, ParamList).and_then(|pl| c.tree.child(pl, ArgList)).is_some_and(|a| {
                    c.tree.children(a).any(|x| c.kind(x) == Arg && c.tree.children(x).next().is_some())
                })
        })
    {
        s.add(
            "SV-DED-015",
            "NeverSetClassParameter",
            Severity::Info,
            "Parameter 'T' of class 'mailbox' is never set by any specialization; every user takes the default. Make it a \
             localparam, or remove it until a second configuration exists and is tested."
                .into(),
            dv_model::Loc::default(),
        );
    }
    for k in generics {
        let gen = &classes.list[k];
        let params = port_params(c, gen.node);
        let mut set: BTreeMap<String, bool> = params.iter().map(|(n, _)| (n.clone(), false)).collect();
        // Every `gen #(...)` in the tree, as written.
        for ty in c.nodes(DataType) {
            if type_name(c, ty).as_deref() != Some(gen.name.as_str()) {
                continue;
            }
            let Some(pl) = c.tree.child(ty, ParamList) else {
                continue;
            };
            let Some(args) = c.tree.child(pl, ArgList) else {
                continue;
            };
            for (ix, a) in c.tree.children(args).filter(|&x| c.kind(x) == Arg).enumerate() {
                let named = c.toks(a).any(|t| c.text_of(t) == ".");
                let pname = if named {
                    c.toks(a)
                        .find(|&t| {
                            let x = c.text_of(t);
                            x != "." && x != "(" && x != ")"
                        })
                        .map(|t| c.text_of(t).to_string())
                } else {
                    params.get(ix).map(|(n, _)| n.clone())
                };
                // `.W()` leaves it at its default.
                let empty = named && c.tree.children(a).next().is_none();
                if let (Some(p), false) = (pname, empty) {
                    if let Some(v) = set.get_mut(&p) {
                        *v = true;
                    }
                }
            }
        }
        for (name, used) in set {
            if used {
                continue;
            }
            let tok = params.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).unwrap();
            s.add(
                "SV-DED-015",
                "NeverSetClassParameter",
                Severity::Info,
                format!(
                    "Parameter '{name}' of class '{}' is never set by any specialization; every user takes the default. Make it a \
                     localparam, or remove it until a second configuration exists and is tested.",
                    gen.name
                ),
                c.loc(tok),
            );
        }
    }
}
