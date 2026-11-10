//! Elaborated copies of code, and routine names and calls. The C++ rules
//! visit slang's AST, so a finding is reported once per elaborated copy of
//! the code around it: per instance of its module and per specialization of
//! its class (see [`copies`]).

use std::collections::HashSet;

use svlang::syntax::NodeId;

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

pub(crate) fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// How many times slang's AST holds the code at `n`: the instances of its
/// module (a module nothing instantiates is a top, once) times, for a
/// parameterized class, its default specialization plus each other
/// parameter list it is used with.
pub(crate) fn copies(c: &Cst, n: NodeId) -> usize {
    let mut k = 1;
    let mut up = c.parent(n);
    while let Some(p) = up {
        match c.kind(p) {
            ModuleDecl | InterfaceDecl | ProgramDecl => k *= module_instances(c, &decl_name(c, p)),
            ClassDecl if c.tree.child(p, Header).and_then(|h| c.tree.child(h, ParamList)).is_some() => {
                k *= 1 + specializations(c, &decl_name(c, p));
            }
            _ => {}
        }
        up = c.parent(p);
    }
    k
}

/// Instances of module `name` in the tree, through the instances of the
/// modules that instantiate it; 1 for a top.
pub(crate) fn module_instances(c: &Cst, name: &str) -> usize {
    fn count(c: &Cst, name: &str, seen: &mut HashSet<String>) -> usize {
        if !seen.insert(name.to_string()) {
            return 1;
        }
        let mut total = 0;
        let mut any = false;
        for inst in c.nodes(Instantiation) {
            let Some(ty) = c.tree.child(inst, DataType) else { continue };
            if c.first_tok(ty).map(|t| c.text_of(t)) != Some(name) {
                continue;
            }
            any = true;
            let here = c.tree.children(inst).filter(|&x| c.kind(x) == Instance).count();
            let mut up = c.parent(inst);
            let mut outer = 1;
            while let Some(p) = up {
                if matches!(c.kind(p), ModuleDecl | InterfaceDecl | ProgramDecl) {
                    outer = count(c, &decl_name(c, p), seen);
                    break;
                }
                up = c.parent(p);
            }
            total += here * outer;
        }
        seen.remove(name);
        if any {
            total
        } else {
            1
        }
    }
    count(c, name, &mut HashSet::new())
}

/// Distinct parameter lists class `name` is written with (`name #(...)`).
pub(crate) fn specializations(c: &Cst, name: &str) -> usize {
    let mut lists = HashSet::new();
    for ty in c.nodes(DataType) {
        if c.first_tok(ty).map(|t| c.text_of(t)) != Some(name) {
            continue;
        }
        let Some(pl) = c.tree.child(ty, ParamList) else { continue };
        let text = c.tree.bare_text(pl);
        if text != "#()" {
            lists.insert(text);
        }
    }
    lists.len()
}

/// slang's `CallExpression`s: calls with an argument list, and names of
/// system functions or known routines used without one. Each comes with
/// its subroutine name and first token (the start of the call).
pub(crate) fn calls(c: &Cst) -> Vec<(NodeId, String, u32)> {
    let mut out = Vec::new();
    for n in c.nodes(CallExpr) {
        let Some(callee) = c.tree.children(n).next() else { continue };
        let Some(first) = c.first_tok(n) else { continue };
        out.push((n, c.last_text(callee).to_string(), first));
    }
    for n in c.nodes(NameExpr).into_iter().chain(c.nodes(ScopedExpr)).chain(c.nodes(MemberExpr)) {
        let callee_of_call = c.parent(n).is_some_and(|p| {
            matches!(c.kind(p), CallExpr | ScopedExpr | MemberExpr) && c.tree.children(p).next() == Some(n)
        });
        if callee_of_call {
            continue;
        }
        let Some(first) = c.first_tok(n) else { continue };
        out.push((n, c.last_text(n).to_string(), first));
    }
    out
}

/// Names of the routines the tree declares.
pub(crate) fn routine_names(c: &Cst) -> HashSet<String> {
    let mut out = HashSet::new();
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)).chain(c.nodes(MethodPrototype)) {
        let name = decl_name(c, n);
        out.insert(name.rsplit("::").next().unwrap_or("").to_string());
    }
    out
}

/// Calls of declared routines named `want`, with their call text.
pub(crate) fn routine_calls(c: &Cst, want: &dyn Fn(&str) -> bool) -> Vec<(NodeId, String, u32)> {
    let declared = routine_names(c);
    calls(c).into_iter().filter(|(_, name, _)| want(name) && declared.contains(name)).collect()
}
