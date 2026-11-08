//! SV-DED-012 (UnreferencedParameter): Flags parameter and localparam
//! declarations that are never referenced within their scope or testbench.

use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// One finding of SV-DED-012: tree, name token, name, scope.
type ParamFinding = (usize, u32, String, String);

/// The scope slang names a parameter's parent: the module, class, package,
/// routine or named generate block (`genblkN` for an unnamed `if`/`case`
/// block, `global` for loop blocks and `$unit`).
fn param_scope(c: &Cst, n: NodeId) -> String {
    for a in crate::support::slang_ast::ancestors(c, n) {
        match c.kind(a) {
            GenerateBlock => {
                if c.parent(a).is_some_and(|p| c.kind(p) == LoopGenerate) {
                    return "global".into();
                }
                if let Some(nm) = c.tree.child(a, Name) {
                    return c.tree.bare_text(nm);
                }
                if let Some(l) = crate::support::slang_ast::block_label_pub(c, a) {
                    return l;
                }
                let Some(p) = c.parent(a) else { return "global".into() };
                if c.kind(p) == LoopGenerate {
                    return "global".into();
                }
                // Unnamed: genblk<N>, N counting the scope's generate constructs.
                let mut top = p;
                while let Some(q) =
                    c.parent(top).filter(|_| matches!(c.kind(top), IfGenerate | CaseGenerate | ElseClause))
                {
                    if matches!(c.kind(q), IfGenerate | ElseClause) {
                        top = q;
                    } else {
                        break;
                    }
                }
                let Some(scope) = c.parent(top) else { return "global".into() };
                let n = kids(c, scope)
                    .into_iter()
                    .filter(|&k| matches!(c.kind(k), IfGenerate | CaseGenerate | LoopGenerate | GenerateRegion))
                    .position(|k| k == top)
                    .map_or(1, |i| i + 1);
                return format!("genblk{n}");
            }
            LoopGenerate => return "global".into(),
            ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | PackageDecl | FunctionDecl | TaskDecl
            | CheckerDecl => {
                return crate::support::slang_ast::last_name(c, a);
            }
            _ => {}
        }
    }
    "global".into()
}

/// Declarations by name: (order, tree, token, scope).
type Decls = HashMap<String, ((usize, usize, u32), usize, u32, String)>;

fn unreferenced_params(c: &Cst) -> std::rc::Rc<Vec<ParamFinding>> {
    thread_local! {
        static FOUND: crate::shared::CompileCache<Vec<ParamFinding>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = FOUND.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    // Declarations by name: (order, tree, token, scope); the one slang
    // visits last is where it reports.
    let mut decls: Decls = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();
    // Classes specialized where slang visits the specialization: a base
    // class, an implemented class, a type parameter's value.
    let mut specialized: HashSet<String> = HashSet::new();
    for t in c.all {
        for i in 0..t.nodes.len() {
            let p = NodeId(i as u32);
            let typed = match t.kind(p) {
                Extends => true,
                ParamDecl => t.token_range(p).iter().any(|&k| &*t.tokens[k as usize].text == "type"),
                _ => false,
            };
            if !typed {
                continue;
            }
            let mut types: Vec<NodeId> = t.children(p).filter(|&d| t.kind(d) == DataType).collect();
            for x in t.children(p).filter(|&x| t.kind(x) == Declarator) {
                types.extend(t.children(x).filter(|&d| t.kind(d) == DataType));
            }
            for d in types {
                for k in t.token_range(d) {
                    specialized.insert(t.tokens[k as usize].text.to_string());
                }
            }
        }
    }
    for (i, t) in c.all.iter().enumerate() {
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, sv_sema::Locator::default());
            own.all = c.all;
            &own
        };
        let visited = |n: NodeId| crate::support::slang_ast::elab_copies(tc, n) != Some(0);
        // Packages, `$unit` and classes first, then instance bodies in
        // elaboration order.
        let rank = |n: NodeId| {
            crate::support::slang_ast::ancestors(tc, n)
                .find(|&a| matches!(tc.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl))
                .map_or(0, |u| {
                    1 + crate::support::slang_ast::last_instance_rank(tc, &crate::support::slang_ast::decl_name(tc, u))
                        .unwrap_or(0)
                })
        };
        let declare = |nm: NodeId, decls: &mut Decls| {
            let Some(tok) = tc.first_tok(nm) else { return };
            if !visited(nm) {
                return;
            }
            let name = tc.tree.bare_text(nm);
            if name.is_empty() || name.starts_with('$') {
                return;
            }
            let order = (rank(nm), i, tok);
            let scope = param_scope(tc, nm);
            let e = decls.entry(name).or_insert((order, i, tok, scope.clone()));
            if order >= e.0 {
                *e = (order, i, tok, scope);
            }
        };
        // A generic class with a parameter lacking a default has no
        // default specialization to visit.
        let no_default = |p: NodeId| {
            let Some(cls) = crate::support::slang_ast::ancestors(tc, p).find(|&a| tc.kind(a) == ClassDecl) else {
                return false;
            };
            let Some(pl) = tc.tree.child(cls, Header).and_then(|h| tc.tree.child(h, ParamList)) else { return false };
            !specialized.contains(&crate::support::slang_ast::decl_name(tc, cls))
                && tc
                    .tree
                    .children(pl)
                    .filter(|&q| tc.kind(q) == ParamDecl)
                    .any(|q| !tc.tree.token_range(q).iter().any(|&t| tc.text_of(t) == "="))
        };
        // `#(type A = int, B = A)`: B is a type parameter too.
        let mut type_params: HashSet<NodeId> = HashSet::new();
        for pl in tc.nodes(ParamList) {
            let mut sticky = false;
            for q in tc.tree.children(pl).filter(|&q| tc.kind(q) == ParamDecl) {
                let toks: Vec<&str> = tc.toks(q).map(|t| tc.text_of(t)).collect();
                if toks.contains(&"type") {
                    sticky = true;
                } else if toks.contains(&"parameter")
                    || toks.contains(&"localparam")
                    || tc.tree.child(q, DataType).is_some_and(|d| {
                        let name = tc
                            .tree
                            .child(q, Name)
                            .or_else(|| tc.tree.child(q, Declarator).and_then(|x| tc.tree.child(x, Name)));
                        tc.first_tok(d) < name.and_then(|n| tc.first_tok(n))
                    })
                {
                    sticky = false;
                }
                if sticky {
                    type_params.insert(q);
                }
            }
        }
        for p in tc.nodes(ParamDecl) {
            let toks: Vec<&str> = tc.toks(p).map(|t| tc.text_of(t)).collect();
            if toks.contains(&"type") || toks.contains(&"specparam") || type_params.contains(&p) || no_default(p) {
                continue;
            }
            let ds: Vec<NodeId> = tc.tree.children(p).filter(|&d| tc.kind(d) == Declarator).collect();
            if ds.is_empty() {
                if let Some(nm) = tc.tree.child(p, Name) {
                    declare(nm, &mut decls);
                }
            }
            for d in ds {
                if let Some(nm) = tc.tree.child(d, Name) {
                    declare(nm, &mut decls);
                }
            }
        }
        for l in tc.nodes(LoopGenerate) {
            if let Some(nm) = tc.tree.child(l, Name) {
                declare(nm, &mut decls);
            }
        }
        // Names slang binds as values, plus those in dimensions and the
        // headers of generate constructs (elaboration evaluates those: a width
        // parameter used only in `logic [W-1:0]` is referenced). Not names in
        // other parts of types.
        let mut roots: HashMap<NodeId, bool> = HashMap::new();
        for e in tc.nodes(NameExpr).into_iter().chain(tc.nodes(ScopedExpr)) {
            if tc.kind(e) == NameExpr && tc.parent(e).is_some_and(|p| tc.kind(p) == ScopedExpr) {
                continue;
            }
            let mut skip = false;
            let mut evaluated = false;
            let mut x = e;
            while let Some(p) = tc.parent(x) {
                match tc.kind(p) {
                    Dim => {
                        evaluated = true;
                        break;
                    }
                    // Parameter values of an instantiation or a base class are bound.
                    DataType
                        if tc.parent(p).is_some_and(|q| {
                            matches!(tc.kind(q), Instantiation | BindDirective | Extends)
                                || (tc.kind(q) == ParamDecl && tc.toks(q).any(|k| tc.text_of(k) == "type"))
                                || (tc.kind(q) == Declarator
                                    && tc.parent(q).is_some_and(|r| {
                                        tc.kind(r) == ParamDecl && tc.toks(r).any(|k| tc.text_of(k) == "type")
                                    }))
                        }) => {}
                    DataType => {
                        skip = true;
                        break;
                    }
                    // The header, not the generated blocks.
                    IfGenerate | CaseGenerate | LoopGenerate if !matches!(tc.kind(x), GenerateBlock | ElseClause) => {
                        evaluated = true;
                        break;
                    }
                    _ => {}
                }
                x = p;
            }
            if skip {
                continue;
            }
            let name = if tc.kind(e) == ScopedExpr { tc.last_text(e).to_string() } else { tc.tree.bare_text(e) };
            if evaluated {
                referenced.insert(name);
                continue;
            }
            // Not in what elaboration skips, nor what slang could not bind.
            if !visited(e) {
                continue;
            }
            let root = crate::support::sema_facts::bind_root(tc, e);
            if !*roots.entry(root).or_insert_with(|| crate::support::sema_facts::root_ok(tc, root))
                || !crate::support::sema_facts::no_bad_expr(tc, e)
            {
                continue;
            }
            referenced.insert(name);
        }
    }
    let mut out: Vec<ParamFinding> = decls
        .into_iter()
        .filter(|(n, _)| {
            !referenced.contains(n)
                && !matches!(n.as_str(), "UVM_NONE" | "UVM_LOW" | "UVM_MEDIUM" | "UVM_HIGH" | "UVM_FULL" | "UVM_DEBUG")
        })
        .map(|(n, (_, t, tok, scope))| (t, tok, n, scope))
        .collect();
    out.sort();
    let v = std::rc::Rc::new(out);
    FOUND.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

pub(crate) fn unreferenced_parameter(c: &Cst, out: &mut Sink) {
    let ix = c.index();
    for (t, tok, name, scope) in unreferenced_params(c).iter().filter(|f| f.0 == ix) {
        let _ = t;
        out.add(
            "SV-DED-012",
            "UnreferencedParameter",
            Severity::Warning,
            format!(
                "Parameter '{name}' is declared in scope '{scope}' but never referenced. Unreferenced parameters complicate \
                 module and VIP interfaces, and often indicate forgotten configuration logic or obsolete constants."
            ),
            c.loc(*tok),
        );
    }
}
