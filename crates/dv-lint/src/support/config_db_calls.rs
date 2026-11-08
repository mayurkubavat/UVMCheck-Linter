//! config_db calls and their field arguments, generate headers and literal
//! shapes, for the type rules.

use sv_syntax::NodeId;

use sv_syntax::SyntaxKind::*;

use crate::support::slang_ast::kids;

use crate::support::style_config::ast_visits;

use crate::syntax::Cst;

/// In a generate construct's header (a loop's bounds, an `if` or `case`
/// condition): elaboration evaluates it, the AST walk does not reach it.
pub(crate) fn in_generate_header(c: &Cst, e: NodeId) -> bool {
    for a in crate::support::slang_ast::ancestors(c, e) {
        match c.kind(a) {
            LoopGenerate | IfGenerate | CaseGenerate => return true,
            GenerateBlock | ElseClause | ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | FunctionDecl
            | TaskDecl | ProceduralBlock | ContinuousAssign | DataDecl | Instantiation => return false,
            _ => {}
        }
    }
    false
}

/// The operand under explicit casts and parentheses, as a rule sees it
/// after stripping conversions.
pub(crate) fn strip(c: &Cst, mut e: NodeId) -> NodeId {
    loop {
        match c.kind(e) {
            ParenExpr => match kids(c, e).first() {
                Some(&i) => e = i,
                None => return e,
            },
            CastExpr => match kids(c, e).get(1) {
                Some(&i) => e = i,
                None => return e,
            },
            _ => return e,
        }
    }
}

pub(crate) fn strip_parens(c: &Cst, mut e: NodeId) -> NodeId {
    while c.kind(e) == ParenExpr {
        match kids(c, e).first() {
            Some(&i) => e = i,
            None => break,
        }
    }
    e
}

/// How often `swe::UserCodeVisitor` reaches `n`: never in library code,
/// each elaborated copy of module code, once in a class (its default
/// specialization).
pub(crate) fn user_visits(c: &Cst, n: NodeId) -> usize {
    if crate::support::slang_ast::is_library(c, n) {
        return 0;
    }
    if crate::support::slang_ast::ancestors(c, n)
        .any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl))
    {
        if let Some(m) = crate::support::slang_ast::elab_copies(c, n) {
            return m;
        }
    }
    crate::support::sva_cov::user_code_times(c, n)
}

/// An expression with the parentheses around it: slang binds `(x)` to `x`
/// with the parenthesized syntax.
pub(crate) fn with_parens(c: &Cst, mut e: NodeId) -> NodeId {
    while let Some(p) = c.parent(e).filter(|&p| c.kind(p) == ParenExpr) {
        e = p;
    }
    e
}

/// 0 or 1 for a 0/1 integer literal (under casts), else None.
pub(crate) fn bool_literal(c: &Cst, e: NodeId) -> Option<i128> {
    let x = strip(c, e);
    if crate::expr_types::slang_kind(c, x) != Some("IntegerLiteral") {
        return None;
    }
    let text = c.tree.bare_text(x).replace('_', "");
    let digits = match text.find('\'') {
        Some(q) => {
            let rest = text[q + 1..].trim_start_matches(['s', 'S']);
            let radix = match rest.as_bytes().first().map(|b| b.to_ascii_lowercase()) {
                Some(b'b') => 2,
                Some(b'o') => 8,
                Some(b'd') => 10,
                Some(b'h') => 16,
                _ => return None,
            };
            i128::from_str_radix(&rest[1..], radix).ok()?
        }
        None => text.parse::<i128>().ok()?,
    };
    matches!(digits, 0 | 1).then_some(digits)
}

/// A `uvm_config_db#(T)::set` or `get` call, as the rules see it.
pub(crate) struct CfgCall {
    pub(crate) set: bool,
    /// The value argument's type as slang prints it: for `set`, the formal
    /// `T` when the value is converted to it.
    pub(crate) ty: String,
    pub(crate) class: String,
    pub(crate) field: String,
    /// The instance-name argument, as [`field_arg`] reads it.
    pub(crate) inst: String,
    pub(crate) loc: dv_model::Loc,
    pub(crate) visits: usize,
    /// The field name is a string literal (not a variable a wrapper
    /// forwards).
    pub(crate) literal: bool,
}

/// The field-name argument: a string literal's value, else its text.
pub(crate) fn field_arg(c: &Cst, a: NodeId) -> String {
    let a = strip(c, a);
    let t = crate::support::type_text::expr_text(c, a);
    let t = t.trim_matches([' ', '\t']);
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return t[1..t.len() - 1].to_string();
    }
    t.to_string()
}

pub(crate) fn config_db_calls(csts: &[Cst]) -> Vec<CfgCall> {
    let mut out = Vec::new();
    // How slang prints the formal's type `T`.
    let in_pkg = csts
        .iter()
        .any(|c| c.nodes(PackageDecl).into_iter().any(|p| crate::support::slang_ast::decl_name(c, p) == "uvm_pkg"));
    let formal = if in_pkg { "uvm_pkg::uvm_config_db::T" } else { "uvm_config_db::T" };
    for c in csts {
        let cfg = crate::expr_types::cfgdb_types(c);
        let ix = c.index();
        for e in c.nodes(CallExpr) {
            let Some((generic, entries)) = cfg.get(&(ix, e)) else { continue };
            let Some((_, name, _)) = crate::support::slang_ast::call_parts(c, e) else { continue };
            if name != "set" && name != "get" {
                continue;
            }
            if !crate::support::sema_facts::ast_ok(c, e) {
                continue;
            }
            let args = crate::expr_types::call_args(c, e);
            let Some(&f) = args.get(2) else { continue };
            let field = field_arg(c, f);
            if field.is_empty() {
                continue;
            }
            let literal = c.kind(strip(c, f)) == Literal && c.tree.bare_text(strip(c, f)).starts_with('"');
            let set = name == "set";
            let v = strip(c, args[3]);
            let unbased = c.kind(v) == Literal && c.tree.bare_text(v).starts_with('\'');
            let class = crate::support::slang_ast::enclosing_class(c, e)
                .map(|k| crate::support::slang_ast::decl_name(c, k))
                .unwrap_or_default();
            let Some(t) = c.first_tok(e) else { continue };
            // A generic class's body: once per specialization, typed there.
            let visits = if *generic { 1 } else { ast_visits(c, e) };
            for (ty, _, converted) in entries {
                let ty = if set && (unbased || *converted) { formal.to_string() } else { ty.text.clone() };
                out.push(CfgCall {
                    set,
                    ty,
                    class: class.clone(),
                    field: field.clone(),
                    inst: args.get(1).map(|&a| field_arg(c, a)).unwrap_or_default(),
                    loc: c.loc(t),
                    visits,
                    literal,
                });
            }
        }
    }
    out
}
