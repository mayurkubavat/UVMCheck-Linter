//! Names, routines, arguments and register blocks read off the syntax tree,
//! for the UVM rules.

use svlang::syntax::{Child, NodeId};

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

/// `n` and everything under it, in source order.
pub(crate) fn case_keyword(c: &Cst, n: NodeId) -> Option<u32> {
    c.toks(n).find(|&t| matches!(c.text_of(t), "case" | "casex" | "casez"))
}

pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.subtree(n)
}

/// A declaration's name (`cls.name`, a routine's last name token).
pub(crate) fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// The Name node of a declaration.
pub(crate) fn name_node(c: &Cst, n: NodeId) -> Option<NodeId> {
    c.tree.child(n, Name)
}

/// Text of a routine's name token: `prototype->name->getLastToken()`.
pub(crate) fn routine_last_name(c: &Cst, f: NodeId) -> String {
    name_node(c, f).map(|x| c.last_text(x).to_string()).unwrap_or_default()
}

/// `prototype->name->getFirstToken()`.
pub(crate) fn routine_first_name(c: &Cst, f: NodeId) -> String {
    name_node(c, f).and_then(|x| c.first_tok(x)).map(|t| c.text_of(t).to_string()).unwrap_or_default()
}

/// Where `prototype->name` starts.
pub(crate) fn name_loc(c: &Cst, f: NodeId) -> Option<dv_model::Loc> {
    name_node(c, f).and_then(|x| c.first_tok(x)).map(|t| c.loc(t))
}

/// A class's `extends ...` clause (slang's `extendsClause`).
pub(crate) fn extends_clause(c: &Cst, cls: NodeId) -> Option<NodeId> {
    let h = c.tree.child(cls, Header)?;
    c.tree.child(h, Extends)
}

/// `extendsClause->baseName`: the base type as written.
pub(crate) fn base_name_node(c: &Cst, cls: NodeId) -> Option<NodeId> {
    c.tree.child(extends_clause(c, cls)?, DataType)
}

/// Functions and tasks declared directly in a class (`cls.items` that are
/// method declarations with a body).
pub(crate) fn class_routines(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree.children(cls).filter(|&n| matches!(c.kind(n), FunctionDecl | TaskDecl)).collect()
}

/// A routine's body items (slang's `func.items`).
pub(crate) fn body_items(c: &Cst, f: NodeId) -> Vec<NodeId> {
    c.tree.child(f, Body).map(|b| c.items(b)).unwrap_or_default()
}

/// slang's `FunctionDeclarationSyntax::toString()`: from the `function` or
/// `task` keyword (qualifiers belong to the enclosing method declaration).
pub(crate) fn routine_text(c: &Cst, f: NodeId) -> String {
    let mut s = String::new();
    let mut on = false;
    for t in c.tree.token_range(f) {
        let tok = &c.tree.tokens[t as usize];
        if !on && matches!(&*tok.text, "function" | "task") {
            on = true;
        }
        if !on {
            continue;
        }
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

/// Drops `//` comments up to their line break (the C++ `stripComments`).
pub(crate) fn strip_line_comments(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut in_line = false;
    let mut i = 0;
    while i < b.len() {
        if !in_line && i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'/' {
            in_line = true;
            i += 1;
        } else if in_line && b[i] == b'\n' {
            in_line = false;
            out.push(b'\n');
        } else if !in_line {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn trim_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}

/// `inv.left->getLastToken().valueText()`: the called name.
pub(crate) fn callee<'t>(c: &Cst<'t>, call: NodeId) -> &'t str {
    c.tree.children(call).next().map(|l| c.last_text(l)).unwrap_or("")
}

/// The call's argument list.
pub(crate) fn arg_list(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.child(call, ArgList)
}

/// slang's argument list parameters: each `Arg` (or an empty slot) in order.
pub(crate) fn params(c: &Cst, list: NodeId) -> Vec<Option<NodeId>> {
    let mut out = Vec::new();
    let mut cur: Option<Option<NodeId>> = None;
    for k in c.kids(list) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" => cur = Some(None),
                "," | ")" => {
                    if let Some(x) = cur.take() {
                        out.push(x);
                    }
                    if c.text_of(t) == "," {
                        cur = Some(None);
                    }
                }
                _ => {}
            },
            Child::Node(n) => {
                if let Some(slot) = cur.as_mut() {
                    if c.kind(n) == Arg {
                        *slot = Some(n);
                    }
                }
            }
        }
    }
    // `f()` has no parameters, not one empty one.
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

/// An ordered argument's expression (not a named `.x(...)` one).
pub(crate) fn ordered_expr(c: &Cst, arg: NodeId) -> Option<NodeId> {
    if c.keyword(arg).is_some_and(|t| c.text_of(t) == ".") {
        return None;
    }
    c.items(arg).into_iter().next()
}

/// The first parameter when it is an ordered argument: its expression.
pub(crate) fn first_ordered(c: &Cst, call: NodeId) -> Option<NodeId> {
    let list = arg_list(c, call)?;
    ordered_expr(c, params(c, list).first().copied()??)
}

/// slang's `AssignmentExpression` (plain `=`): left and right.
pub(crate) fn assignment(c: &Cst, n: NodeId) -> Option<(u32, NodeId, NodeId)> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.toks(n).next()?;
    if c.text_of(op) != "=" {
        return None;
    }
    let mut sides = c.tree.children(n);
    Some((op, sides.next()?, sides.next()?))
}

/// The expression of an expression statement.
pub(crate) fn stmt_expr(c: &Cst, s: NodeId) -> Option<NodeId> {
    (c.kind(s) == ExprStmt).then(|| c.items(s).into_iter().next()).flatten()
}

/// Innermost class declaration around `n`.
pub(crate) fn enclosing_class(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut cur = c.parent(n);
    while let Some(p) = cur {
        if c.kind(p) == ClassDecl {
            return Some(p);
        }
        cur = c.parent(p);
    }
    None
}

pub(crate) fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// RAL rules' register block test on a class declaration.
pub(crate) fn is_reg_block_decl(c: &Cst, cls: NodeId) -> bool {
    let l = lower(&decl_name(c, cls));
    l.contains("reg_block")
        || l.contains("reg_model")
        || extends_clause(c, cls).is_some_and(|e| c.text(e).contains("uvm_reg_block"))
}

/// `build()` routines declared directly in register block classes.
pub(crate) fn reg_block_builds(c: &Cst) -> Vec<(String, NodeId)> {
    let mut out = Vec::new();
    for cls in c.nodes(ClassDecl) {
        if !is_reg_block_decl(c, cls) {
            continue;
        }
        let name = decl_name(c, cls);
        for f in class_routines(c, cls) {
            if name_node(c, f).is_some() && routine_first_name(c, f) == "build" {
                out.push((name.clone(), f));
            }
        }
    }
    out
}

pub(crate) fn routines(c: &Cst) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)).collect();
    v.sort_by_key(|n| n.0);
    v
}
