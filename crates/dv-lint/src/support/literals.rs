//! Literals, instantiations and loop iteration counts, for the language
//! gotcha rules.
//!
//! The literal rules visit slang's AST, which holds a literal once per
//! elaborated copy of the code around it
//! ([`sv_sema::Elaborated::copies`]) and leaves out what elaboration folds
//! away: dimensions, generate conditions and loop headers, and parameter
//! defaults every instance overrides.

use sv_syntax::{NodeId, SyntaxKind, TokenKind};

use std::collections::HashMap;

use crate::support::statements::Units;

use crate::support::sema_facts::{ast_ok, is_expr};

use crate::syntax::Cst;

use SyntaxKind::*;

/// An integer literal as slang reads it: whether a size was written, the
/// base (None for a plain decimal) and the digits.
pub(crate) struct Lit {
    pub(crate) sized: bool,
    pub(crate) based: bool,
    pub(crate) digits: String,
}

pub(crate) fn integer_literal(c: &Cst, n: NodeId) -> Option<Lit> {
    let toks: Vec<u32> = c.tree.token_range(n);
    let kinds: Vec<TokenKind> = toks.iter().map(|&t| c.tree.tokens[t as usize].kind).collect();
    match kinds.as_slice() {
        [TokenKind::IntegerLiteral] => Some(Lit { sized: false, based: false, digits: c.text_of(toks[0]).to_string() }),
        [TokenKind::IntegerBase, TokenKind::VectorDigits | TokenKind::IntegerLiteral] => {
            Some(Lit { sized: false, based: true, digits: c.text_of(toks[1]).to_string() })
        }
        [TokenKind::IntegerLiteral, TokenKind::IntegerBase, TokenKind::VectorDigits | TokenKind::IntegerLiteral] => {
            Some(Lit { sized: true, based: true, digits: c.text_of(toks[2]).to_string() })
        }
        _ => None,
    }
}

/// The literal's text as slang's `toString()` with the whitespace removed.
pub(crate) fn compact(c: &Cst, n: NodeId) -> String {
    c.text(n).chars().filter(|ch| !ch.is_whitespace()).collect()
}

pub(crate) fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// The parameter a declaration names, and whether every instantiation of
/// its module overrides it (slang then never binds the default).
pub(crate) fn overridden_everywhere(c: &Cst, unit: NodeId, param: NodeId) -> Option<usize> {
    let module = decl_name(c, unit);
    let Some(name) = c.tree.child(param, Declarator).and_then(|d| c.tree.child(d, Name)).map(|x| c.tree.bare_text(x))
    else {
        return None;
    };
    // Its position among the unit's header parameters, for positional lists.
    let header =
        c.tree.child(unit, ParamList).or_else(|| c.tree.child(unit, Header).and_then(|h| c.tree.child(h, ParamList)));
    let pos = header.and_then(|h| c.tree.children(h).filter(|&p| c.kind(p) == ParamDecl).position(|p| p == param));
    let insts = instantiations(c);
    let recs = insts.get(&module)?;
    // An instantiation leaves the default bound unless its parameter list
    // sets it (slang binds the default before it applies a defparam).
    let overrides = |list: &Option<String>| {
        let Some(text) = list else { return false };
        let named = text.contains(&format!(".{name}("));
        let positional = !text.contains("(.")
            && pos.is_some_and(|p| text.trim_start_matches("#(").trim_end_matches(')').split(',').count() > p);
        named || positional
    };
    let Some(paths) = crate::support::slang_ast::instances_of(c, &module) else {
        return recs.iter().all(|r| overrides(&r.1)).then_some(0);
    };
    let bound = paths
        .iter()
        .filter(|(parent, inst)| {
            let Some(parent) = parent else { return true };
            let rec = recs.iter().find(|r| &r.0 == parent && r.2.contains(inst));
            !rec.is_some_and(|r| overrides(&r.1))
        })
        .count();
    Some(bound)
}

/// Every instantiation of each module in the compile: the module it is in,
/// its parameter list (compacted) and its instance names.
pub(crate) type Insts = HashMap<String, Vec<(String, Option<String>, Vec<String>)>>;

pub(crate) fn instantiations(c: &Cst) -> std::rc::Rc<Insts> {
    thread_local! {
        static INSTS: std::cell::RefCell<Option<((usize, usize), std::rc::Rc<Insts>)>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = INSTS.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let mut out: Insts = HashMap::new();
    for (i, t) in c.all.iter().enumerate() {
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, sv_sema::Locator::default());
            own.all = c.all;
            &own
        };
        // `bind target m #(...) u (...);` instantiates m in target.
        let mut sites = tc.nodes(Instantiation);
        sites.extend(tc.nodes(BindDirective));
        for inst in sites {
            let Some(ty) = tc.tree.child(inst, DataType) else { continue };
            let Some(m) = tc.first_tok(ty).map(|t| tc.text_of(t).to_string()) else { continue };
            let list = tc.tree.child(ty, ParamList).or_else(|| tc.tree.child(inst, ParamList)).map(|l| compact(tc, l));
            let names: Vec<String> = tc
                .tree
                .children(inst)
                .filter(|&k| tc.kind(k) == Instance)
                .filter_map(|k| tc.tree.child(k, Name).map(|nm| tc.tree.bare_text(nm)))
                .collect();
            let parent = if tc.kind(inst) == BindDirective {
                tc.tree.child(inst, NameExpr).map(|t| tc.tree.bare_text(t))
            } else {
                std::iter::successors(tc.parent(inst), |&a| tc.parent(a))
                    .find(|&a| matches!(tc.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl))
                    .map(|u| decl_name(tc, u))
            };
            out.entry(m).or_default().push((parent.unwrap_or_default(), list, names));
        }
    }
    let v = std::rc::Rc::new(out);
    INSTS.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

/// Some module of the compile instantiates `module`.
pub(crate) fn is_instantiated(c: &Cst, module: &str) -> bool {
    instantiations(c).contains_key(module)
}

/// The module instance and specialization counts of the tree (cached).
pub(crate) fn units(c: &Cst) -> std::sync::Arc<Units> {
    crate::shared::get("units", c.tree_key(), || Units::new(c))
}

/// How many times slang's AST holds the literal `n` (0 where elaboration
/// folds it away).
pub(crate) fn ast_times(c: &Cst, n: NodeId) -> usize {
    let mut x = n;
    let mut loops = 1usize;
    let mut class_param = false;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            // Dimensions and enum values are not expressions in slang's
            // AST, nor are sequence delays and repetitions, clocking skews,
            // gate and specify delays, defparam values, or randsequence
            // productions.
            Dim | Repetition | ClockingDecl | GateInst | SpecifyBlock | RsCodeBlock => return 0,
            // `extends base(args)` arguments are not bound as written.
            ArgList if c.parent(p).is_some_and(|q| c.kind(q) == Extends) => return 0,
            // The parameters of a base that does not resolve.
            Extends
                if c.parent(p).and_then(|h| c.parent(h)).is_some_and(|cls| {
                    c.kind(cls) == ClassDecl
                        && c.tree.child(cls, Name).is_some_and(|nm| {
                            crate::support::sema_facts::info(c).tainted_classes.contains(&c.tree.bare_text(nm))
                        })
                }) =>
            {
                return 0
            }
            // A base's parameters bind once.
            Extends => class_param = true,
            ParamDecl if c.keyword(p).is_some_and(|k| c.text_of(k) == "defparam") => return 0,
            TimingControl
                if c.tok(p, "##").is_some()
                    && c.parent(p).is_some_and(|q| matches!(c.kind(q), SequenceExpr | PropertyExpr)) =>
            {
                return 0
            }
            // Sequence, property and let formals and actuals are
            // substituted into the body.
            PortList if c.parent(p).is_some_and(|q| matches!(c.kind(q), SequenceDecl | PropertyDecl | LetDecl)) => {
                return 0
            }
            ArgList if c.parent(p).is_some_and(|call| c.kind(call) == CallExpr && is_instance_call(c, call)) => {
                return 0
            }

            DataType if c.tok(p, "enum").is_some() => return 0,
            // `virtual bus #(8) vif`: matched against instances, not bound.
            DataType if c.first_tok(p).is_some_and(|t| c.text_of(t) == "virtual") => return 0,
            // `'{3{1}}` for a struct: the count is a constant.
            AssignPattern
                if c.tree.children(p).next() == Some(x)
                    && c.tree.child(p, ConcatExpr).is_some()
                    && struct_target(c, p) =>
            {
                return 0
            }
            // A streaming concatenation's slice size is a constant, not an
            // operand.
            ConcatExpr if is_streaming(c, p) && c.kind(x) != ConcatExpr => return 0,
            // A generate condition or loop header, not its blocks.
            IfGenerate | LoopGenerate | CaseGenerate if is_expr(c.kind(x)) => return 0,
            LoopGenerate => loops = loops.saturating_mul(loop_iterations(c, p).unwrap_or(1)),
            ParamDecl if c.tree.child(p, Declarator).is_some() => {
                let unit = std::iter::successors(c.parent(p), |&a| c.parent(a))
                    .find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | PackageDecl));
                match unit.map(|u| (u, c.kind(u))) {
                    // Only a parameter an instance can set: not a localparam,
                    // nor one in a generate block.
                    Some((u, ModuleDecl | InterfaceDecl | ProgramDecl))
                        if c.keyword(p).is_some_and(|k| c.text_of(k) != "localparam")
                            && c.parent(p).is_some_and(|q| q == u || c.kind(q) == ParamList) =>
                    {
                        if let Some(k) = overridden_everywhere(c, u, p) {
                            return if k > 0 && ast_ok(c, n) { k } else { 0 };
                        }
                    }
                    // A class parameter's default binds in its default
                    // specialization only.
                    Some((_, ClassDecl)) if c.parent(p).is_some_and(|q| c.kind(q) == ParamList) => class_param = true,
                    _ => {}
                }
            }
            _ => {}
        }
        x = p;
    }
    if !ast_ok(c, n) {
        return 0;
    }
    if class_param {
        return loops;
    }
    // Module code: the elaborated hierarchy has each copy, generate
    // constructs unrolled and untaken branches left out.
    if let Some(m) = crate::support::slang_ast::elab_copies(c, n) {
        return m.saturating_mul(units(c).spec_times(c, n));
    }
    units(c).times(c, n).saturating_mul(loops)
}

/// The declared type of the variable an assignment pattern initializes is
/// a struct.
pub(crate) fn struct_target(c: &Cst, ap: NodeId) -> bool {
    let Some(dcl) = c.parent(ap).filter(|&d| c.kind(d) == Declarator) else { return false };
    let Some(dt) = c.parent(dcl).and_then(|d| c.tree.child(d, DataType)) else { return false };
    if c.tok(dt, "struct").is_some() {
        return true;
    }
    let name = c.tree.bare_text(dt);
    c.nodes(TypedefDecl).into_iter().any(|t| {
        c.tree.child(t, Name).is_some_and(|n| c.tree.bare_text(n) == name)
            && c.tree.child(t, DataType).is_some_and(|d| c.tok(d, "struct").is_some())
    })
}

/// Iterations of `for (genvar i = A; i op B; step)` with literal bounds.
pub(crate) fn loop_iterations(c: &Cst, lg: NodeId) -> Option<usize> {
    let int = |n: NodeId| -> Option<i64> {
        let n = unparen(c, n);
        let lit = (c.kind(n) == Literal).then(|| integer_literal(c, n)).flatten()?;
        if lit.based {
            return None;
        }
        lit.digits.replace('_', "").parse().ok()
    };
    let kids: Vec<NodeId> = c.tree.children(lg).collect();
    let var = c.tree.bare_text(*kids.iter().find(|&&k| c.kind(k) == Name)?);
    let exprs: Vec<NodeId> = kids.iter().copied().filter(|&k| is_expr(c.kind(k))).collect();
    let [init, cond, step] = exprs[..] else { return None };
    let mut i = int(init)?;
    let ck: Vec<NodeId> = c.tree.children(cond).collect();
    let [cl, cr] = ck[..] else { return None };
    if c.kind(cl) != NameExpr || c.tree.bare_text(cl) != var {
        return None;
    }
    let bound = int(cr)?;
    let cop = ["<=", ">=", "!=", "<", ">"].into_iter().find(|o| c.tok(cond, o).is_some())?;
    let delta: i64 = if c.tok(step, "++").is_some() {
        1
    } else if c.tok(step, "--").is_some() {
        -1
    } else {
        let sk: Vec<NodeId> = c.tree.children(step).collect();
        let v = int(*sk.get(1)?)?;
        if c.tok(step, "+=").is_some() {
            v
        } else if c.tok(step, "-=").is_some() {
            -v
        } else {
            return None;
        }
    };
    let mut n = 0usize;
    while n < 1 << 16 {
        let go = match cop {
            "<" => i < bound,
            "<=" => i <= bound,
            ">" => i > bound,
            ">=" => i >= bound,
            _ => i != bound,
        };
        if !go {
            return Some(n);
        }
        n += 1;
        i += delta;
    }
    None
}

pub(crate) fn unparen(c: &Cst, mut n: NodeId) -> NodeId {
    while c.kind(n) == ParenExpr {
        match c.tree.children(n).next() {
            Some(k) => n = k,
            None => break,
        }
    }
    n
}

/// A call of a sequence, property or let declared in the tree.
pub(crate) fn is_instance_call(c: &Cst, call: NodeId) -> bool {
    let Some(callee) = c.tree.children(call).next() else { return false };
    let name = c.last_text(callee);
    [SequenceDecl, PropertyDecl, LetDecl].into_iter().any(|k| c.nodes(k).into_iter().any(|d| decl_name(c, d) == name))
}

/// `{<< n {...}}` / `{>> {...}}`.
pub(crate) fn is_streaming(c: &Cst, n: NodeId) -> bool {
    c.tok(n, "<<").is_some() || c.tok(n, ">>").is_some()
}
