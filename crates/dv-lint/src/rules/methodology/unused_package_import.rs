//! SV-PKG-003 (UnusedPackageImport): Flags package imports that supply no name
//! used in their scope.

use crate::support::defines_packages::{import_items, user_tok, Files};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::{HashMap, HashSet};
use svlang::lexer::Kind;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The names each package declares at its top (what a wildcard import of
/// it can supply).
fn package_exports(csts: &[Cst]) -> HashMap<String, HashSet<String>> {
    let Some(first) = csts.first() else { return HashMap::new() };
    let scopes = svlang::semantic::scopes::build(first.all);
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for sym in &scopes.symbols {
        let sc = &scopes.scopes[sym.scope];
        if sc.symbol.is_some_and(|o| scopes.symbols[o].kind == svlang::semantic::scopes::SymKind::Package) {
            out.entry(sc.path.clone()).or_default().insert(sym.name.clone());
        }
    }
    for sc in &scopes.scopes {
        if let Some(o) = sc.symbol.filter(|&o| scopes.symbols[o].kind == svlang::semantic::scopes::SymKind::Package) {
            out.entry(scopes.symbols[o].name.clone()).or_default();
        }
    }
    out
}

/// The names a range of tokens looks up without a scope prefix: not after
/// `::` or `.`, not a package name before `::`, and not declared there.
/// The token bounds of each `Name` directly under a node other than a
/// `DataType` or `Name`, by first token.
fn declared_names(c: &Cst) -> Vec<(u32, u32)> {
    let t = c.tree;
    let mut out = Vec::new();
    for n in t.preorder() {
        if c.kind(n) == DataType || c.kind(n) == Name {
            continue;
        }
        for nm in t.children(n).filter(|&x| c.kind(x) == Name) {
            out.extend(t.token_bounds(nm));
        }
    }
    out.sort_unstable();
    out
}

fn looked_up(
    c: &Cst,
    lo: u32,
    hi: u32,
    packages: &HashMap<String, HashSet<String>>,
    skip: &[(u32, u32)],
) -> HashSet<String> {
    let t = c.tree;
    let mut declared = HashSet::new();
    let mut decl_toks = HashSet::new();
    let names = crate::shared::get("declared_names", c.tree_key(), || declared_names(c));
    let from = names.partition_point(|&(a, _)| a < lo);
    for &(a, b) in names[from..].iter().take_while(|&&(a, _)| a <= hi) {
        if a >= lo && b <= hi {
            decl_toks.insert(b);
            declared.insert(c.text_of(b).to_string());
        }
    }
    let mut out = HashSet::new();
    for i in lo..=hi {
        if skip.iter().any(|&(a, b)| i >= a && i <= b) {
            continue;
        }
        let tok = &t.tokens[i as usize];
        if tok.kind != Kind::Identifier || decl_toks.contains(&i) {
            continue;
        }
        let prev = (i > 0).then(|| &*t.tokens[i as usize - 1].text);
        if matches!(prev, Some("::" | ".")) {
            continue;
        }
        let next = t.tokens.get(i as usize + 1).map(|x| &*x.text);
        if next == Some("::") && packages.contains_key(&*tok.text) {
            continue;
        }
        if !declared.contains(&*tok.text) {
            out.insert(tok.text.to_string());
        }
    }
    out
}

/// Whether a wildcard import of `pkg` in the same scope as import `n`
/// (design unit or package `unit`, or the unit) comes before it.
fn earlier_import(csts: &[Cst], c: &Cst, n: NodeId, unit: NodeId, pkg: &str) -> bool {
    let Some((start, _)) = c.tree.token_bounds(n) else { return false };
    let at_unit = c.kind(unit) == SourceFile;
    csts.iter().filter(|o| if at_unit { o.index() <= c.index() } else { o.index() == c.index() }).any(|o| {
        o.nodes(ImportDecl).into_iter().any(|i| {
            if !import_items(o, i).iter().any(|&(p, it)| o.text_of(p) == pkg && o.text_of(it) == "*") {
                return false;
            }
            let Some(u) = crate::support::slang_ast::ancestors(o, i)
                .find(|&a| matches!(o.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile))
            else {
                return false;
            };
            let same = if at_unit { o.kind(u) == SourceFile } else { u == unit };
            same && (o.index() < c.index() || o.tree.token_bounds(i).is_some_and(|(a, _)| a < start))
        })
    })
}

pub(crate) fn unused_package_import(_: &Design, csts: &[Cst], out: &mut Sink) {
    let packages = package_exports(csts);
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c);
        for n in c.nodes(ImportDecl) {
            let Some(kw) = c.keyword(n) else { continue };
            if c.text_of(kw) != "import" || !user_tok(c, &fs, kw) {
                continue;
            }
            // The scope the import is in: a design unit or package, or $unit.
            let Some(unit) = crate::support::slang_ast::ancestors(c, n)
                .find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile))
            else {
                continue;
            };
            let at_unit = c.kind(unit) == SourceFile;
            // slang's root holds the elaborated instances only.
            // An import in a design unit is a symbol of each instance body.
            let copies = if matches!(c.kind(unit), ModuleDecl | InterfaceDecl | ProgramDecl) {
                crate::support::slang_ast::elab_copies(c, n).unwrap_or(1)
            } else {
                1
            };
            if copies == 0 {
                continue;
            }
            let where_ = if at_unit {
                "$unit".to_string()
            } else {
                c.tree.child(unit, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
            };
            let Some((_, end)) = c.tree.token_bounds(n) else { continue };
            for (pt, item) in import_items(c, n) {
                let pkg = c.text_of(pt).to_string();
                let Some(exports) = packages.get(&pkg) else { continue };
                let o = c.tree.tokens[item as usize].origin;
                if !seen.insert((fs.path(o.file), o.offset, where_.clone())) {
                    continue;
                }
                let name = c.text_of(item);
                // slang keeps only the first wildcard import of a package in
                // a scope (the unit spans every file).
                if name == "*" && earlier_import(csts, c, n, unit, &pkg) {
                    continue;
                }
                let supplies = |names: &HashSet<String>| {
                    if name == "*" {
                        names.iter().any(|x| exports.contains(x))
                    } else {
                        names.contains(name)
                    }
                };
                let used = if at_unit {
                    // Everything after it in the unit, but design units that
                    // import the package themselves.
                    // Every lookup after it in the unit (later imports of the
                    // package there are dropped as duplicates).
                    let mut used = false;
                    for o in csts.iter().filter(|o| o.index() >= c.index()) {
                        let Some((lo, hi)) = o.tree.token_bounds(o.tree.root) else { continue };
                        let lo = if o.index() == c.index() { end + 1 } else { lo };
                        // Design units no instance elaborates look nothing up.
                        let mut skip: Vec<(u32, u32)> = [ModuleDecl, InterfaceDecl, ProgramDecl]
                            .into_iter()
                            .flat_map(|k| o.nodes(k))
                            .filter(|&u| crate::support::slang_ast::elab_copies(o, u) == Some(0))
                            .filter_map(|u| o.tree.token_bounds(u))
                            .collect();
                        for i in o.nodes(ImportDecl) {
                            if !import_items(o, i).iter().any(|&(p, _)| o.text_of(p) == pkg) {
                                continue;
                            }
                            match crate::support::slang_ast::ancestors(o, i).find(|&a| {
                                matches!(o.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile)
                            }) {
                                Some(u) if o.kind(u) == SourceFile => {}
                                Some(u) => skip.extend(o.tree.token_bounds(u)),
                                None => {}
                            }
                        }
                        if lo <= hi && supplies(&looked_up(o, lo, hi, &packages, &skip)) {
                            used = true;
                            break;
                        }
                    }
                    used
                } else {
                    let Some((_, hi)) = c.tree.token_bounds(unit) else { continue };
                    end < hi && supplies(&looked_up(c, end + 1, hi, &packages, &[]))
                };
                if used {
                    continue;
                }
                let what = format!("{pkg}::{name}");
                for _ in 0..copies {
                    out.add(
                    "SV-PKG-003",
                    "UnusedPackageImport",
                    Severity::Warning,
                    format!(
                        "'import {what}' in {where_} supplies nothing that is used there. Remove it, so the scope's dependencies are the ones it really has."
                    ),
                    c.loc(item),
                );
                }
            }
        }
    }
}
