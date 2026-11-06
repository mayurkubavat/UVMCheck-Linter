//! Helpers of the group C rules (concurrency, SVA, coverage): what the C++
//! rules get from slang's AST that a syntax tree only implies.
//!
//! - [`times`]: how often slang's AST walk reaches a node: once per instance
//!   of its module (generate loops unrolled) and once per specialization of
//!   its parameterized class.
//! - [`lookup`]: the declaration a simple name resolves to, enough to tell a
//!   class property from a local and to read a declared type.
//! - [`user_routines`] and [`user_classes`]: `swe::allUserRoutines` and
//!   `swe::userClasses`.
//! - [`std_unordered_order`]: the iteration order of a libstdc++
//!   `std::unordered_set<std::string>`, which some messages list names in.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use sv_syntax::{NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::syntax::Cst;

/// libstdc++'s `std::hash<std::string>` (`_Hash_bytes`, seed 0xc70f6907).
pub(crate) fn std_hash(b: &[u8]) -> u64 {
    const MUL: u64 = (0xc6a4_a793u64 << 32) + 0x5bd1_e995;
    let shift_mix = |v: u64| v ^ (v >> 47);
    let len = b.len();
    let mut hash = 0xc70f_6907u64 ^ (len as u64).wrapping_mul(MUL);
    let aligned = len & !7;
    for ch in b[..aligned].chunks(8) {
        let data = u64::from_le_bytes(ch.try_into().unwrap());
        let data = shift_mix(data.wrapping_mul(MUL)).wrapping_mul(MUL);
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    if len & 7 != 0 {
        let mut data = 0u64;
        for &x in b[aligned..].iter().rev() {
            data = (data << 8) + x as u64;
        }
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    hash = shift_mix(hash).wrapping_mul(MUL);
    shift_mix(hash)
}

/// The order a libstdc++ `std::unordered_set<std::string>` iterates after
/// inserting `items` in turn (duplicates dropped); `None` past the sizes
/// modelled here.
pub(crate) fn std_unordered_order<S: AsRef<str>>(items: &[S]) -> Option<Vec<String>> {
    const PRIMES: &[usize] = &[29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97, 103, 109, 113, 127,
        137, 139, 149, 157, 167, 179, 193, 199, 211, 227, 241, 257, 277, 293, 313, 337, 359, 383, 409, 439, 467,
        503, 541];
    const FAST: [usize; 14] = [2, 2, 2, 3, 5, 5, 7, 7, 11, 11, 11, 11, 13, 13];
    #[derive(Clone, Copy)]
    enum Before {
        Head,
        Node(usize),
    }
    let mut keys: Vec<String> = Vec::new();
    let mut hashes: Vec<u64> = Vec::new();
    let mut next: Vec<Option<usize>> = Vec::new();
    let mut head: Option<usize> = None;
    let mut buckets: Vec<Option<Before>> = vec![None; 1];
    let mut next_resize = 0usize;
    for it in items {
        let k = it.as_ref();
        if keys.iter().any(|x| x == k) {
            continue;
        }
        let n_elt = keys.len();
        let n_bkt = buckets.len();
        if n_elt + 1 > next_resize {
            let min_bkts = (n_elt + 1).max(if next_resize != 0 { 0 } else { 11 });
            if min_bkts >= n_bkt {
                let want = (min_bkts + 1).max(n_bkt * 2);
                let nb = if want < FAST.len() {
                    next_resize = FAST[want];
                    FAST[want]
                } else {
                    let p = *PRIMES.iter().find(|&&p| p >= want)?;
                    if p == 541 {
                        return None;
                    }
                    next_resize = p;
                    p
                };
                // Rehash.
                let mut nb_v: Vec<Option<Before>> = vec![None; nb];
                let mut p = head;
                head = None;
                let mut bbegin = 0usize;
                while let Some(i) = p {
                    let nx = next[i];
                    let b = (hashes[i] % nb as u64) as usize;
                    match nb_v[b] {
                        None => {
                            next[i] = head;
                            head = Some(i);
                            nb_v[b] = Some(Before::Head);
                            if next[i].is_some() {
                                nb_v[bbegin] = Some(Before::Node(i));
                            }
                            bbegin = b;
                        }
                        Some(bf) => {
                            let after = match bf {
                                Before::Head => head,
                                Before::Node(j) => next[j],
                            };
                            next[i] = after;
                            match bf {
                                Before::Head => head = Some(i),
                                Before::Node(j) => next[j] = Some(i),
                            }
                        }
                    }
                    p = nx;
                }
                buckets = nb_v;
            } else {
                next_resize = n_bkt;
            }
        }
        let i = keys.len();
        let h = std_hash(k.as_bytes());
        keys.push(k.to_string());
        hashes.push(h);
        next.push(None);
        let nb = buckets.len();
        let b = (h % nb as u64) as usize;
        match buckets[b] {
            Some(bf) => {
                let after = match bf {
                    Before::Head => head,
                    Before::Node(j) => next[j],
                };
                next[i] = after;
                match bf {
                    Before::Head => head = Some(i),
                    Before::Node(j) => next[j] = Some(i),
                }
            }
            None => {
                next[i] = head;
                head = Some(i);
                if let Some(n) = next[i] {
                    let nbk = (hashes[n] % nb as u64) as usize;
                    buckets[nbk] = Some(Before::Node(i));
                }
                buckets[b] = Some(Before::Head);
            }
        }
    }
    let mut out = Vec::new();
    let mut p = head;
    while let Some(i) = p {
        out.push(keys[i].clone());
        p = next[i];
    }
    Some(out)
}

// ---- tree walking ----

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// `n` and every node under it, in source order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        out.push(x);
        let k: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(k.into_iter().rev());
    }
    out
}

/// The nodes above `n`, nearest first.
pub(crate) fn ancestors<'a>(c: &'a Cst, n: NodeId) -> impl Iterator<Item = NodeId> + 'a {
    std::iter::successors(c.parent(n), move |&p| c.parent(p))
}

/// A declaration's name as written (`c::f` for an out-of-block method).
pub(crate) fn decl_name(c: &Cst, decl: NodeId) -> String {
    c.tree.child(decl, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// The last identifier of a declaration's name (`f` of `c::f`).
pub(crate) fn last_name(c: &Cst, decl: NodeId) -> String {
    let full = decl_name(c, decl);
    full.rsplit("::").next().unwrap_or("").to_string()
}

/// A token's leading trivia as slang has it: after a macro expansion, only
/// what follows the macro call (its text is not trivia).
pub(crate) fn tok_trivia<'t>(c: &Cst<'t>, t: u32) -> &'t str {
    let tok = &c.tree.tokens[t as usize];
    let triv = c.tree.trivia(t);
    if t == 0 {
        return triv;
    }
    let prev = &c.tree.tokens[t as usize - 1];
    if !prev.from_macro || tok.from_macro || prev.origin.file != tok.origin.file {
        return triv;
    }
    let src: &'t str = &c.tree.sources[tok.origin.file as usize];
    let use_at = prev.origin.offset as usize;
    if use_at < tok.trivia_start as usize || use_at >= tok.origin.offset as usize {
        return triv;
    }
    let end = macro_call_end(src, use_at).min(tok.origin.offset as usize);
    &src[end..tok.origin.offset as usize]
}

/// The end of a macro call written at `at`: the backtick, the name and a
/// balanced argument list if one follows.
fn macro_call_end(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let mut i = at + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
        i += 1;
    }
    let mut j = i;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    if j >= b.len() || b[j] != b'(' {
        return i;
    }
    let mut depth = 0i32;
    let mut k = j;
    while k < b.len() {
        match b[k] {
            b'"' => {
                k += 1;
                while k < b.len() && b[k] != b'"' {
                    if b[k] == b'\\' {
                        k += 1;
                    }
                    k += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
        k += 1;
    }
    i
}

/// slang's `toString()` of the tokens `a..=b`: each with its leading trivia.
pub(crate) fn span_text(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(tok_trivia(c, t));
        } else if t != a && tok.space_before {
            // slang keeps a macro body's trivia on the expanded tokens.
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// slang's `toString()` of a node.
pub(crate) fn slang_text(c: &Cst, n: NodeId) -> String {
    match c.tree.token_bounds(n) {
        Some((a, b)) => span_text(c, a, b),
        None => String::new(),
    }
}

/// [`slang_text`] with surrounding whitespace trimmed.
pub(crate) fn slang_trimmed(c: &Cst, n: NodeId) -> String {
    slang_text(c, n).trim_matches([' ', '\t', '\n', '\r']).to_string()
}

/// `swe::normalizedText`: the tokens with any trivia between them made one
/// space.
pub(crate) fn norm_text(c: &Cst, n: NodeId) -> String {
    let mut s = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if tok.text.is_empty() {
            continue;
        }
        if !s.is_empty() && (tok.space_before || !tok_trivia(c, t).is_empty()) {
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// The keyword tokens of a routine declaration before its name or return
/// type (`function`, `automatic`, `static`, `virtual`, `extern`, ...).
pub(crate) fn has_kw(c: &Cst, n: NodeId, kw: &str) -> bool {
    c.toks(n).any(|t| c.text_of(t) == kw)
}

/// The class a node is in: its class declaration, or for the body of an
/// out-of-block method (`function void c::f()`), the class it names.
pub(crate) fn enclosing_class(c: &Cst, n: NodeId) -> Option<NodeId> {
    for a in std::iter::once(n).chain(ancestors(c, n)) {
        match c.kind(a) {
            ClassDecl => return Some(a),
            FunctionDecl | TaskDecl => {
                let full = decl_name(c, a);
                if let Some((cls, _)) = full.rsplit_once("::") {
                    return find_class(c, cls.rsplit("::").next().unwrap_or(cls), a);
                }
            }
            _ => {}
        }
    }
    None
}

// ---- per-tree facts, computed once ----

#[derive(Default)]
struct TreeInfo {
    /// Instance paths of each definition.
    instances: HashMap<String, Vec<String>>,
    /// Generate block paths (`top.gl[0]`).
    generates: HashMap<String, Vec<String>>,
    /// Class declarations by name.
    classes: HashMap<String, Vec<NodeId>>,
    /// Routines declared outside classes, by name.
    routines: HashMap<String, Vec<NodeId>>,
    /// Specializations of each parameterized class (its declaration).
    specs: HashMap<u32, usize>,
}

thread_local! {
    static INFO: RefCell<Option<(usize, usize, std::rc::Rc<TreeInfo>)>> = const { RefCell::new(None) };
}

fn info(c: &Cst) -> std::rc::Rc<TreeInfo> {
    let key = (c.tree as *const Tree as usize, c.tree.nodes.len());
    INFO.with(|cell| {
        if let Some((a, b, i)) = &*cell.borrow() {
            if (*a, *b) == key {
                return i.clone();
            }
        }
        let i = std::rc::Rc::new(build_info(c));
        *cell.borrow_mut() = Some((key.0, key.1, i.clone()));
        i
    })
}

fn build_info(c: &Cst) -> TreeInfo {
    let mut ti = TreeInfo::default();
    for n in c.nodes(ClassDecl) {
        ti.classes.entry(decl_name(c, n)).or_default().push(n);
    }
    for k in [FunctionDecl, TaskDecl] {
        for n in c.nodes(k) {
            if enclosing_class_decl(c, n).is_none() && !decl_name(c, n).contains("::") {
                ti.routines.entry(decl_name(c, n)).or_default().push(n);
            }
        }
    }
    let needs_hier = c.nodes(ModuleDecl).len() + c.nodes(InterfaceDecl).len() + c.nodes(ProgramDecl).len() > 0;
    if needs_hier {
        for line in sv_sema::hierarchy(std::slice::from_ref(c.tree)) {
            let mut it = line.splitn(3, ' ');
            let (Some(parent), Some(kind), Some(rest)) = (it.next(), it.next(), it.next()) else { continue };
            let path = |name: &str| if parent == "$root" { name.to_string() } else { format!("{parent}.{name}") };
            match kind {
                "instance" => {
                    if let Some((name, def)) = rest.split_once(" : ") {
                        ti.instances.entry(def.to_string()).or_default().push(path(name));
                    }
                }
                "generate" => ti.generates.entry(parent.to_string()).or_default().push(path(rest)),
                _ => {}
            }
        }
    }
    let generic: Vec<NodeId> = c.nodes(ClassDecl).into_iter().filter(|&n| class_params(c, n).is_some()).collect();
    if !generic.is_empty() {
        let names: HashMap<String, NodeId> = generic.iter().map(|&n| (decl_name(c, n), n)).collect();
        let mut keys: HashMap<u32, HashSet<Vec<String>>> = HashMap::new();
        for n in c.nodes(ParamList) {
            let Some(p) = c.parent(n) else { continue };
            if !matches!(c.kind(p), DataType | NameExpr) {
                continue;
            }
            let Some(t) = c.first_tok(p) else { continue };
            let Some(&cls) = names.get(c.text_of(t)) else { continue };
            if let Some(k) = spec_key(c, cls, Some(n)) {
                keys.entry(cls.0).or_default().insert(k);
            }
        }
        for &cls in &generic {
            let default = spec_key(c, cls, None);
            let set = keys.get(&cls.0);
            let extra = set.map_or(0, |s| s.iter().filter(|k| Some(*k) != default.as_ref()).count());
            ti.specs.insert(cls.0, 1 + extra);
        }
    }
    ti
}

fn enclosing_class_decl(c: &Cst, n: NodeId) -> Option<NodeId> {
    ancestors(c, n).find(|&a| c.kind(a) == ClassDecl)
}

/// A parameterized class's parameters: name and default (as written).
fn class_params(c: &Cst, cls: NodeId) -> Option<Vec<(String, String)>> {
    let h = c.tree.child(cls, Header)?;
    let pl = c.tree.child(h, ParamList)?;
    let mut out = Vec::new();
    for p in c.tree.children(pl).filter(|&p| c.kind(p) == ParamDecl) {
        if has_kw(c, p, "type") {
            let name = c.tree.child(p, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
            let def = c.tree.child(p, DataType).map(|x| c.tree.bare_text(x)).unwrap_or_default();
            out.push((name, def));
        } else {
            for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
                let name = c.tree.child(d, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                let def = kids(c, d).into_iter().filter(|&k| c.kind(k) != Name).last().map(|x| c.tree.bare_text(x)).unwrap_or_default();
                out.push((name, def));
            }
        }
    }
    Some(out)
}

/// The parameter values of a specialization of `cls` (`None` for its
/// default), with the parameters of a generic class around the reference
/// replaced by their defaults, as in that class's default specialization.
fn spec_key(c: &Cst, cls: NodeId, list: Option<NodeId>) -> Option<Vec<String>> {
    let params = class_params(c, cls)?;
    let mut vals: Vec<String> = params.iter().map(|p| p.1.clone()).collect();
    if let Some(pl) = list {
        let outer: Vec<(String, String)> = ancestors(c, pl)
            .filter(|&a| c.kind(a) == ClassDecl)
            .filter_map(|a| class_params(c, a))
            .flatten()
            .collect();
        let subst = |t: &str| -> String {
            match outer.iter().find(|o| o.0 == t) {
                Some(o) => o.1.clone(),
                None => t.to_string(),
            }
        };
        let args = c.tree.child(pl, ArgList)?;
        for (i, a) in c.tree.children(args).filter(|&a| c.kind(a) == Arg).enumerate() {
            let named = c.toks(a).nth(1).map(|t| c.text_of(t).to_string());
            let val = kids(c, a).first().map(|&x| {
                let toks = c.tree.token_range(x);
                toks.iter().map(|&t| subst(c.text_of(t))).collect::<String>()
            });
            let val = val.unwrap_or_default();
            let ix = match named {
                Some(n) if c.toks(a).next().is_some_and(|t| c.text_of(t) == ".") => params.iter().position(|p| p.0 == n)?,
                _ => i,
            };
            if ix < vals.len() {
                vals[ix] = val;
            }
        }
    }
    Some(vals)
}

/// How many times slang's AST walk visits `n`: once per instance of the
/// module, interface or program it is in (with generate loops unrolled),
/// times the specializations of the parameterized class it is in.
pub(crate) fn times(c: &Cst, n: NodeId) -> usize {
    let ti = info(c);
    let chain: Vec<NodeId> = {
        let mut v: Vec<NodeId> = std::iter::once(n).chain(ancestors(c, n)).collect();
        v.reverse();
        v
    };
    let mut count = 1usize;
    let mut paths: Option<Vec<String>> = None;
    for (i, &a) in chain.iter().enumerate() {
        match c.kind(a) {
            ModuleDecl | InterfaceDecl | ProgramDecl if paths.is_none() => {
                let name = decl_name(c, a);
                paths = Some(ti.instances.get(&name).cloned().unwrap_or_else(|| vec![name]));
            }
            LoopGenerate => {
                let label = chain.get(i + 1).and_then(|&b| block_label(c, b));
                if let (Some(ps), Some(l)) = (paths.as_mut(), label) {
                    let mut next = Vec::new();
                    for p in ps.iter() {
                        let pre = format!("{p}.{l}[");
                        if let Some(g) = ti.generates.get(p) {
                            next.extend(g.iter().filter(|x| x.starts_with(&pre)).cloned());
                        }
                    }
                    if !next.is_empty() {
                        *ps = next;
                    }
                }
            }
            IfGenerate | CaseGenerate => {
                let label = chain.get(i + 1).and_then(|&b| block_label(c, b));
                if let (Some(ps), Some(l)) = (paths.as_mut(), label) {
                    for p in ps.iter_mut() {
                        let q = format!("{p}.{l}");
                        if ti.generates.get(p).is_some_and(|g| g.contains(&q)) {
                            *p = q;
                        }
                    }
                }
            }
            ClassDecl => count *= ti.specs.get(&a.0).copied().unwrap_or(1),
            _ => {}
        }
    }
    count * paths.map_or(1, |p| p.len().max(1))
}

/// A generate block's label (`begin : l` or `l : begin`).
fn block_label(c: &Cst, b: NodeId) -> Option<String> {
    if c.kind(b) != GenerateBlock {
        return None;
    }
    let l = c.tree.child(b, EndLabel).or_else(|| c.tree.child(b, StmtLabel))?;
    c.toks(l).map(|t| c.text_of(t)).find(|t| *t != ":").map(str::to_string)
}

// ---- classes and routines ----

/// A class declaration by name (the one in the same scope as `at` first).
pub(crate) fn find_class(c: &Cst, name: &str, at: NodeId) -> Option<NodeId> {
    let ti = info(c);
    let v = ti.classes.get(name)?;
    let scope = ancestors(c, at).find(|&a| matches!(c.kind(a), PackageDecl | ModuleDecl | InterfaceDecl | ProgramDecl));
    v.iter().copied().find(|&x| ancestors(c, x).any(|a| Some(a) == scope)).or_else(|| v.first().copied())
}

/// The base class named in a class's `extends` (its last name: `c` of
/// `p::c #(T)`).
pub(crate) fn base_name(c: &Cst, cls: NodeId) -> Option<String> {
    let h = c.tree.child(cls, Header)?;
    let e = c.tree.child(h, Extends)?;
    if c.keyword(e).is_some_and(|k| c.text_of(k) != "extends") {
        return None;
    }
    let d = c.tree.child(e, DataType)?;
    c.toks(d).map(|t| c.text_of(t)).filter(|t| *t != "::").last().map(str::to_string)
}

/// The class and its ancestors that are in the tree, nearest first.
pub(crate) fn class_chain(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    let mut out = vec![cls];
    let mut cur = cls;
    while out.len() < 64 {
        let Some(b) = base_name(c, cur) else { break };
        let Some(x) = find_class(c, &b, cur) else { break };
        if out.contains(&x) {
            break;
        }
        out.push(x);
        cur = x;
    }
    out
}

/// The class's name and those of its ancestors (resolved or not): what
/// the first `uvm_*` base, and so the class's role, is read from.
pub(crate) fn base_names(c: &Cst, cls: NodeId) -> Vec<String> {
    let chain = class_chain(c, cls);
    let mut out: Vec<String> = chain.iter().skip(1).map(|&x| decl_name(c, x)).collect();
    if let Some(b) = chain.last().and_then(|&l| base_name(c, l)) {
        if !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

/// `swe::isLibrarySymbol` for a node: in package `uvm_pkg`, in a class named
/// `uvm_*`, or in uvm_pkg.sv or the test mock.
pub(crate) fn is_library(c: &Cst, n: NodeId) -> bool {
    for a in std::iter::once(n).chain(ancestors(c, n)) {
        match c.kind(a) {
            PackageDecl if decl_name(c, a) == "uvm_pkg" => return true,
            ClassDecl if decl_name(c, a).starts_with("uvm_") => return true,
            _ => {}
        }
    }
    let Some(t) = c.first_tok(n) else { return false };
    let f = c.tree.tokens[t as usize].origin.file as usize;
    let path = c.tree.files.get(f).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    path.contains("uvm_mock.svh") || path.contains("uvm_pkg.sv")
}

/// `UvmModel::getAllClasses`: classes in packages and `$unit` (also nested
/// ones), not parameterized ones and not those in modules.
pub(crate) fn model_classes(c: &Cst) -> Vec<NodeId> {
    c.nodes(ClassDecl)
        .into_iter()
        .filter(|&n| class_params(c, n).is_none())
        .filter(|&n| !ancestors(c, n).any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl | ParamList | FunctionDecl | TaskDecl)))
        .collect()
}

/// `swe::userClasses`: model classes plus parameterized ones (their default
/// specialization), without library classes. Generic classes nested in a
/// class are not reached.
pub(crate) fn user_classes(c: &Cst) -> Vec<NodeId> {
    c.nodes(ClassDecl)
        .into_iter()
        .filter(|&n| !ancestors(c, n).any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl | FunctionDecl | TaskDecl)))
        .filter(|&n| class_params(c, n).is_none() || enclosing_class_decl(c, n).is_none())
        .filter(|&n| !is_library(c, n))
        .collect()
}

/// `swe::classMethods`: the class's own methods with bodies, an extern
/// method's body being the out-of-block declaration.
pub(crate) fn class_methods(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    let name = decl_name(c, cls);
    let mut out = Vec::new();
    for m in kids(c, cls) {
        match c.kind(m) {
            FunctionDecl | TaskDecl => out.push(m),
            MethodPrototype => {
                if has_kw(c, m, "pure") || has_kw(c, m, "import") {
                    continue;
                }
                let mname = decl_name(c, m);
                if let Some(body) = out_of_block(c, &name, &mname, cls) {
                    out.push(body);
                }
            }
            _ => {}
        }
    }
    out
}

fn out_of_block(c: &Cst, cls: &str, m: &str, at: NodeId) -> Option<NodeId> {
    [FunctionDecl, TaskDecl].into_iter().flat_map(|k| c.nodes(k)).find(|&d| {
        let full = decl_name(c, d);
        let mut parts = full.rsplit("::");
        parts.next() == Some(m) && parts.next() == Some(cls) && enclosing_class(c, d).is_some_and(|x| x == at)
    })
}

/// `swe::allUserRoutines`: the methods of user classes, then routines of
/// packages, `$unit` and module instances (once per instance).
pub(crate) fn user_routines(c: &Cst) -> Vec<(NodeId, usize)> {
    let mut out: Vec<(NodeId, usize)> = Vec::new();
    let mut seen = HashSet::new();
    for cls in user_classes(c) {
        for m in class_methods(c, cls) {
            if seen.insert(m) {
                out.push((m, 1));
            }
        }
    }
    for k in [FunctionDecl, TaskDecl] {
        for r in c.nodes(k) {
            if seen.contains(&r) || decl_name(c, r).contains("::") || is_library(c, r) {
                continue;
            }
            let p = c.parent(r).map(|p| c.kind(p));
            let n = match p {
                Some(SourceFile | PackageDecl) => 1,
                Some(ModuleDecl | InterfaceDecl | ProgramDecl) => times(c, r),
                _ => continue,
            };
            seen.insert(r);
            out.push((r, n));
        }
    }
    out
}

// ---- names ----

/// What a simple name was declared as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DeclKind {
    /// In a block or routine body.
    Local,
    Argument,
    /// A class property (of the class or an ancestor).
    Property,
    /// A variable or net of a module, interface, package or `$unit`.
    Static,
    Parameter,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Decl {
    pub kind: DeclKind,
    /// The declaration's `DataType`, when one is written.
    pub dtype: Option<NodeId>,
    /// The `Declarator` (or the parameter declaration).
    pub node: NodeId,
}

fn declarators_named(c: &Cst, decl: NodeId, name: &str) -> Option<NodeId> {
    c.tree
        .children(decl)
        .filter(|&d| c.kind(d) == Declarator)
        .find(|&d| c.tree.child(d, Name).is_some_and(|x| c.tree.bare_text(x) == name))
}

fn in_items(c: &Cst, scope: NodeId, name: &str, kind: DeclKind) -> Option<Decl> {
    for d in kids(c, scope) {
        match c.kind(d) {
            DataDecl => {
                if let Some(x) = declarators_named(c, d, name) {
                    return Some(Decl { kind, dtype: c.tree.child(d, DataType), node: x });
                }
            }
            ParamDecl => {
                if has_kw(c, d, "type") {
                    if c.tree.child(d, Name).is_some_and(|x| c.tree.bare_text(x) == name) {
                        return Some(Decl { kind: DeclKind::Parameter, dtype: None, node: d });
                    }
                } else if let Some(x) = declarators_named(c, d, name) {
                    return Some(Decl { kind: DeclKind::Parameter, dtype: c.tree.child(d, DataType), node: x });
                }
            }
            Header => {
                for k in kids(c, d) {
                    if matches!(c.kind(k), PortList | ParamList) {
                        if let Some(x) = in_items(c, k, name, kind) {
                            return Some(x);
                        }
                        if let Some(x) = in_ports(c, k, name, kind) {
                            return Some(x);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn in_ports(c: &Cst, list: NodeId, name: &str, kind: DeclKind) -> Option<Decl> {
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        if let Some(x) = declarators_named(c, p, name) {
            return Some(Decl { kind, dtype: c.tree.child(p, DataType), node: x });
        }
    }
    None
}

/// Properties of a class and its ancestors in the tree.
pub(crate) fn class_member(c: &Cst, cls: NodeId, name: &str) -> Option<Decl> {
    for k in class_chain(c, cls) {
        if let Some(d) = in_items(c, k, name, DeclKind::Property) {
            return Some(d);
        }
    }
    None
}

/// The declaration `name` names at `at`, looking out from the innermost
/// block: block locals, routine arguments, class properties (also
/// inherited), then module, package and `$unit` items.
pub(crate) fn lookup(c: &Cst, at: NodeId, name: &str) -> Option<Decl> {
    for a in ancestors(c, at) {
        match c.kind(a) {
            SeqBlock | ParBlock | Body | LoopStmt | GenerateBlock => {
                if let Some(d) = in_items(c, a, name, DeclKind::Local) {
                    return Some(d);
                }
            }
            FunctionDecl | TaskDecl => {
                if let Some(pl) = c.tree.child(a, PortList) {
                    if let Some(d) = in_ports(c, pl, name, DeclKind::Argument) {
                        return Some(d);
                    }
                }
                if decl_name(c, a).contains("::") {
                    if let Some(cls) = enclosing_class(c, a) {
                        if let Some(d) = class_member(c, cls, name) {
                            return Some(d);
                        }
                    }
                }
            }
            ClassDecl => {
                if let Some(d) = class_member(c, a, name) {
                    return Some(d);
                }
                if let Some(h) = c.tree.child(a, Header) {
                    if let Some(pl) = c.tree.child(h, ParamList) {
                        if let Some(d) = in_items(c, pl, name, DeclKind::Parameter) {
                            return Some(d);
                        }
                    }
                }
            }
            ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile | CheckerDecl => {
                if let Some(d) = in_items(c, a, name, DeclKind::Static) {
                    return Some(d);
                }
            }
            _ => {}
        }
    }
    None
}

/// The type an expression has, as the name of its type's first identifier
/// (`semaphore`, a class name): for names, `this`, and member chains.
pub(crate) fn expr_type(c: &Cst, e: NodeId) -> Option<String> {
    match c.kind(e) {
        NameExpr => {
            let t = c.first_tok(e)?;
            let name = c.text_of(t);
            if name == "this" {
                return enclosing_class(c, e).map(|k| decl_name(c, k));
            }
            let d = lookup(c, e, name)?;
            let t = c.first_tok(d.dtype?)?;
            Some(c.text_of(t).to_string())
        }
        MemberExpr => {
            let base = *kids(c, e).first()?;
            let member = c.last_text(e).to_string();
            let bt = expr_type(c, base)?;
            let cls = find_class(c, &bt, e)?;
            let d = class_member(c, cls, &member)?;
            let t = c.first_tok(d.dtype?)?;
            Some(c.text_of(t).to_string())
        }
        ParenExpr => expr_type(c, *kids(c, e).first()?),
        _ => None,
    }
}

/// A call: its callee expression, name, and receiver (`r` of `r.f()`).
pub(crate) fn call_parts(c: &Cst, call: NodeId) -> Option<(NodeId, String, Option<NodeId>)> {
    let callee = *kids(c, call).first()?;
    match c.kind(callee) {
        NameExpr => {
            let t = c.first_tok(callee)?;
            Some((callee, c.text_of(t).to_string(), None))
        }
        MemberExpr => {
            let recv = *kids(c, callee).first()?;
            Some((callee, c.last_text(callee).to_string(), Some(recv)))
        }
        ScopedExpr => Some((callee, c.last_text(callee).to_string(), None)),
        _ => None,
    }
}

/// The routine a call names, if it is declared in the tree: a method of the
/// receiver's class (or of the class around), or a free routine.
pub(crate) fn callee_decl(c: &Cst, call: NodeId) -> Option<NodeId> {
    let (callee, name, recv) = call_parts(c, call)?;
    let cls = match recv {
        Some(r) => {
            let t = c.first_tok(r)?;
            if c.text_of(t) == "super" && c.kind(r) == NameExpr {
                enclosing_class(c, call).and_then(|k| class_chain(c, k).get(1).copied())
            } else {
                let ty = expr_type(c, r)?;
                Some(find_class(c, &ty, call)?)
            }
        }
        None if c.kind(callee) == ScopedExpr => {
            let first = kids(c, callee).first().and_then(|&x| c.first_tok(x)).map(|t| c.text_of(t).to_string())?;
            find_class(c, &first, call)
        }
        None => enclosing_class(c, call),
    };
    if let Some(cls) = cls {
        for k in class_chain(c, cls) {
            for m in kids(c, k) {
                if matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) && last_name(c, m) == name {
                    return Some(m);
                }
            }
        }
        if recv.is_some() {
            return None;
        }
    }
    let ti = info(c);
    ti.routines.get(&name).and_then(|v| v.first().copied())
}

/// The tokens directly under a node, as text.
pub(crate) fn tok_texts<'a>(c: &'a Cst, n: NodeId) -> Vec<&'a str> {
    c.toks(n).map(|t| c.text_of(t)).collect()
}
