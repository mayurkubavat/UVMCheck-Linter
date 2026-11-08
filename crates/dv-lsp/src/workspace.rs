//! All indexed files, a name table over them, and name resolution:
//! scopes, class inheritance, `pkg::`/`Class::` and `obj.member` chains.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::index::{FileIndex, SymKind, Symbol};
use crate::text::{is_ident_byte, word_at};

/// A symbol: file id and index in that file.
pub type SymRef = (usize, u32);

#[derive(Default)]
pub struct Workspace {
    pub files: Vec<Option<FileIndex>>,
    pub by_path: HashMap<PathBuf, usize>,
    /// Name -> every non-local symbol with that name.
    pub globals: HashMap<String, Vec<SymRef>>,
}

pub const SV_EXTENSIONS: &[&str] = &["sv", "svh", "v", "vh", "svi", "svp"];
const SKIP_DIRS: &[&str] = &["node_modules", "target", "__pycache__", "out", "dist"];

impl Workspace {
    pub fn sym(&self, r: SymRef) -> &Symbol {
        &self.files[r.0].as_ref().expect("live file").symbols[r.1 as usize]
    }

    pub fn file(&self, f: usize) -> &FileIndex {
        self.files[f].as_ref().expect("live file")
    }

    pub fn file_id(&self, path: &Path) -> Option<usize> {
        self.by_path
            .get(path)
            .copied()
            .filter(|&f| self.files[f].is_some())
    }

    fn unlink(&mut self, f: usize) {
        if let Some(old) = self.files[f].take() {
            for s in old.symbols.iter().filter(|s| !s.local) {
                if let Some(v) = self.globals.get_mut(&s.name) {
                    v.retain(|r| r.0 != f);
                    if v.is_empty() {
                        self.globals.remove(&s.name);
                    }
                }
            }
        }
    }

    pub fn insert(&mut self, idx: FileIndex) -> usize {
        let f = match self.by_path.get(&idx.path) {
            Some(&f) => {
                self.unlink(f);
                f
            }
            None => {
                self.files.push(None);
                let f = self.files.len() - 1;
                self.by_path.insert(idx.path.clone(), f);
                f
            }
        };
        for (i, s) in idx.symbols.iter().enumerate() {
            if !s.local {
                self.globals
                    .entry(s.name.clone())
                    .or_default()
                    .push((f, i as u32));
            }
        }
        self.files[f] = Some(idx);
        f
    }

    pub fn update(&mut self, path: &Path, text: String) -> usize {
        self.insert(FileIndex::build(path, text))
    }

    pub fn remove(&mut self, path: &Path) {
        if let Some(&f) = self.by_path.get(path) {
            self.unlink(f);
        }
    }

    /// Indexes every SV file under the roots, in parallel.
    pub fn scan(&mut self, roots: &[PathBuf], exclude: &[String]) -> usize {
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        for r in roots {
            collect(r, exclude, &mut paths, &mut seen, 0);
        }
        paths.retain(|p| !self.by_path.contains_key(p));
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16);
        let chunk = paths.len().div_ceil(n.max(1)).max(1);
        let built: Vec<FileIndex> = std::thread::scope(|s| {
            let handles: Vec<_> = paths
                .chunks(chunk)
                .map(|ps| {
                    s.spawn(move || {
                        ps.iter()
                            .filter_map(|p| {
                                let bytes = std::fs::read(p).ok()?;
                                if bytes.len() > 8 << 20 {
                                    return None;
                                }
                                let text = String::from_utf8_lossy(&bytes).into_owned();
                                // A parser bug must not take the server down.
                                std::panic::catch_unwind(|| FileIndex::build(p, text)).ok()
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        let count = built.len();
        for idx in built {
            self.insert(idx);
        }
        count
    }

    // ---- resolution ----

    fn named(&self, name: &str) -> &[SymRef] {
        self.globals.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn parent_of(&self, r: SymRef) -> Option<SymRef> {
        self.sym(r).parent.map(|p| (r.0, p))
    }

    /// Classes (and other scopes) called `name`; a typedef to one is followed.
    pub fn scopes_named(&self, name: &str, kinds: &[SymKind]) -> Vec<SymRef> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut todo = vec![(name.to_string(), 0)];
        while let Some((n, depth)) = todo.pop() {
            if depth > 4 || !seen.insert(n.clone()) {
                continue;
            }
            for &r in self.named(&n) {
                let s = self.sym(r);
                if kinds.contains(&s.kind) {
                    out.push(r);
                } else if s.kind == SymKind::Typedef {
                    if let Some(t) = &s.type_name {
                        todo.push((t.clone(), depth + 1));
                    }
                }
            }
        }
        out
    }

    /// Direct members of a scope, then (for a class) inherited ones.
    pub fn members(&self, scope: SymRef) -> Vec<SymRef> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        self.members_into(scope, &mut out, &mut seen, 0);
        out
    }

    fn members_into(
        &self,
        scope: SymRef,
        out: &mut Vec<SymRef>,
        seen: &mut HashSet<SymRef>,
        depth: usize,
    ) {
        if depth > 24 || !seen.insert(scope) {
            return;
        }
        let file = self.file(scope.0);
        for (i, s) in file.children(Some(scope.1)) {
            if !s.local {
                out.push((scope.0, i));
                // Enum values are visible in the scope holding the typedef.
                if s.kind == SymKind::Typedef {
                    out.extend(
                        file.children(Some(i))
                            .filter(|(_, e)| e.kind == SymKind::EnumMember)
                            .map(|(j, _)| (scope.0, j)),
                    );
                }
            }
        }
        let s = self.sym(scope);
        if s.kind == SymKind::Class {
            if let Some(base) = s.type_name.clone() {
                for b in self
                    .scopes_named(&base, &[SymKind::Class])
                    .into_iter()
                    .take(1)
                {
                    self.members_into(b, out, seen, depth + 1);
                }
            }
        }
    }

    /// Out-of-block bodies `function T C::name` for a class member.
    pub fn bodies_of(&self, class: &str, name: &str) -> Vec<SymRef> {
        self.named(name)
            .iter()
            .copied()
            .filter(|&r| self.sym(r).scope_class.as_deref() == Some(class))
            .collect()
    }

    /// The class a method body belongs to: its parent, or `C` of `C::f`.
    fn class_of_method(&self, m: SymRef) -> Option<SymRef> {
        let s = self.sym(m);
        if let Some(c) = &s.scope_class {
            return self.scopes_named(c, &[SymKind::Class]).first().copied();
        }
        let p = self.parent_of(m)?;
        (self.sym(p).kind == SymKind::Class).then_some(p)
    }

    /// The scopes around `offset`, innermost first, with the class of an
    /// out-of-block method put after the method.
    pub fn scope_chain(&self, f: usize, offset: u32) -> Vec<SymRef> {
        let file = self.file(f);
        let mut out = Vec::new();
        for i in file.enclosing(offset).into_iter().rev() {
            let r = (f, i);
            out.push(r);
            if self.sym(r).scope_class.is_some() {
                if let Some(c) = self.class_of_method(r) {
                    out.push(c);
                }
            }
        }
        out
    }

    pub fn enclosing_class(&self, f: usize, offset: u32) -> Option<SymRef> {
        self.scope_chain(f, offset)
            .into_iter()
            .find(|&r| self.sym(r).kind == SymKind::Class)
    }

    /// Everything visible by plain name at `offset`, innermost first
    /// (locals, the scope's members with inherited ones, file level,
    /// imported packages). Names repeat when shadowed.
    pub fn visible(&self, f: usize, offset: u32) -> Vec<SymRef> {
        let mut out = Vec::new();
        let file = self.file(f);
        for scope in self.scope_chain(f, offset) {
            if scope.0 == f
                && matches!(
                    self.sym(scope).kind,
                    SymKind::Function | SymKind::Task | SymKind::Covergroup | SymKind::Clocking
                )
            {
                // Ports and locals declared before the cursor.
                for (i, s) in file.children(Some(scope.1)) {
                    if s.start <= offset || s.kind == SymKind::Port {
                        out.push((f, i));
                    }
                }
            } else {
                out.extend(self.members(scope));
            }
        }
        for (i, s) in file.children(None) {
            out.push((f, i));
            if s.kind == SymKind::Typedef {
                out.extend(file.children(Some(i)).map(|(j, _)| (f, j)));
            }
        }
        for pkg in &file.imports {
            for p in self.scopes_named(pkg, &[SymKind::Package]) {
                out.extend(self.members(p));
            }
        }
        out
    }

    /// The scope a symbol's value or type leads to (for `x.` / `X::`).
    pub fn target_of(&self, r: SymRef) -> Option<SymRef> {
        let s = self.sym(r);
        match s.kind {
            SymKind::Class
            | SymKind::Package
            | SymKind::Module
            | SymKind::Interface
            | SymKind::Program
            | SymKind::Checker => Some(r),
            SymKind::Typedef => self
                .scopes_named(&s.name, &[SymKind::Class])
                .first()
                .copied(),
            SymKind::Variable
            | SymKind::Port
            | SymKind::Parameter
            | SymKind::Instance
            | SymKind::Function => {
                let t = s.type_name.as_ref()?;
                self.scopes_named(
                    t,
                    &[
                        SymKind::Class,
                        SymKind::Module,
                        SymKind::Interface,
                        SymKind::Program,
                    ],
                )
                .first()
                .copied()
            }
            _ => None,
        }
    }

    /// Resolves a chain of names (`a`, `.b`, `::c`) to the scope its last
    /// part leads to.
    pub fn resolve_chain(
        &self,
        f: usize,
        offset: u32,
        chain: &[(Sep, String)],
        trailing: Sep,
    ) -> Option<SymRef> {
        let mut cur: Option<SymRef> = None;
        for (k, (sep, name)) in chain.iter().enumerate() {
            let next_sep = chain.get(k + 1).map(|c| c.0).or(Some(trailing));
            cur = match cur {
                None => match name.as_str() {
                    "this" => self.enclosing_class(f, offset),
                    "super" => {
                        let c = self.enclosing_class(f, offset)?;
                        let base = self.sym(c).type_name.clone()?;
                        self.scopes_named(&base, &[SymKind::Class]).first().copied()
                    }
                    _ if next_sep == Some(Sep::Scope) && *sep == Sep::None => {
                        let mut s = self.scopes_named(name, &[SymKind::Package, SymKind::Class]);
                        if s.is_empty() {
                            s = self
                                .visible(f, offset)
                                .into_iter()
                                .filter(|&r| self.sym(r).name == *name)
                                .filter_map(|r| self.target_of(r))
                                .collect();
                        }
                        s.first().copied()
                    }
                    _ => {
                        let r = self
                            .visible(f, offset)
                            .into_iter()
                            .find(|&r| self.sym(r).name == *name)
                            .or_else(|| {
                                self.named(name)
                                    .iter()
                                    .copied()
                                    .find(|&r| self.target_of(r).is_some())
                            })?;
                        self.target_of(r)
                    }
                },
                Some(scope) => {
                    if name == "type_id" || name == "super" {
                        Some(scope)
                    } else {
                        let m = self
                            .members(scope)
                            .into_iter()
                            .find(|&r| self.sym(r).name == *name)?;
                        self.target_of(m)
                    }
                }
            };
            cur?;
        }
        cur
    }

    /// The declarations the name at `offset` may refer to, best first.
    pub fn definitions(&self, f: usize, offset: u32) -> Vec<SymRef> {
        let file = self.file(f);
        let Some((s, e)) = word_at(&file.text, offset) else {
            return Vec::new();
        };
        let word = &file.text[s..e];
        if let Some(m) = word.strip_prefix('`') {
            return self
                .named(m)
                .iter()
                .copied()
                .filter(|&r| self.sym(r).kind == SymKind::Macro)
                .collect();
        }
        // On a declaration's own name: that declaration, plus the other
        // half of an extern/out-of-block method pair.
        if let Some(i) = file
            .symbols
            .iter()
            .position(|sy| sy.sel_start as usize == s && sy.sel_end as usize == e)
        {
            let me = (f, i as u32);
            let sy = self.sym(me);
            let mut out = vec![me];
            if let Some(c) = &sy.scope_class {
                for cl in self.scopes_named(c, &[SymKind::Class]) {
                    out.extend(self.members(cl).into_iter().filter(|&r| {
                        self.sym(r).name == sy.name
                            && r.0 == cl.0
                            && self.sym(r).parent == Some(cl.1)
                    }));
                }
            } else if let Some(p) = self
                .parent_of(me)
                .filter(|&p| self.sym(p).kind == SymKind::Class)
            {
                out.extend(self.bodies_of(&self.sym(p).name, &sy.name));
            }
            return out;
        }
        let chain = chain_before(&file.text, s);
        if !chain.is_empty() {
            let sep = chain_sep(&file.text, s);
            if let Some(scope) = self.resolve_chain(f, offset, &chain, sep) {
                let mut hits: Vec<SymRef> = self
                    .members(scope)
                    .into_iter()
                    .filter(|&r| self.sym(r).name == word)
                    .collect();
                let extra: Vec<SymRef> = hits
                    .iter()
                    .filter_map(|&r| self.parent_of(r).map(|p| (p, self.sym(r).name.clone())))
                    .flat_map(|(p, n)| self.bodies_of(&self.sym(p).name.clone(), &n))
                    .collect();
                hits.extend(extra);
                if !hits.is_empty() {
                    return hits;
                }
            }
            // Unresolved receiver: any class member of that name.
            let members: Vec<SymRef> = self
                .named(word)
                .iter()
                .copied()
                .filter(|&r| {
                    self.parent_of(r)
                        .is_some_and(|p| self.sym(p).kind.is_scope())
                        || self.sym(r).scope_class.is_some()
                })
                .collect();
            if !members.is_empty() {
                return members;
            }
        }
        let vis: Vec<SymRef> = self
            .visible(f, offset)
            .into_iter()
            .filter(|&r| self.sym(r).name == word)
            .collect();
        if let Some(&first) = vis.first() {
            let mut out = vec![first];
            if let Some(p) = self
                .parent_of(first)
                .filter(|&p| self.sym(p).kind == SymKind::Class)
            {
                out.extend(self.bodies_of(&self.sym(p).name, word));
            }
            return out;
        }
        // Anything else of that name; types and units first.
        let mut all: Vec<SymRef> = self.named(word).to_vec();
        all.sort_by_key(|&r| {
            let s = self.sym(r);
            (!s.kind.is_type(), s.parent.is_some(), r.0 != f)
        });
        all
    }
}

/// SV files under `dir`, skipping hidden, build-output and excluded
/// directories (an exclude matches a directory name or a path suffix).
fn collect(
    dir: &Path,
    exclude: &[String],
    out: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) {
    if depth > 32 || out.len() >= 50_000 {
        return;
    }
    let Ok(canon) = dir.canonicalize() else {
        return;
    };
    if !seen.insert(canon) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        let is_dir = ft.is_dir() || (ft.is_symlink() && p.is_dir());
        if is_dir {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let ps = p.to_string_lossy();
            if exclude
                .iter()
                .any(|x| !x.is_empty() && (name == *x || ps.ends_with(x.as_str())))
            {
                continue;
            }
            collect(&p, exclude, out, seen, depth + 1);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| SV_EXTENSIONS.contains(&x))
        {
            out.push(p);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sep {
    None,
    Dot,
    Scope,
}

/// The separator right before byte `s` (skipping spaces): `.` or `::`.
pub fn chain_sep(text: &str, s: usize) -> Sep {
    let b = text.as_bytes();
    let mut i = s;
    while i > 0 && b[i - 1] == b' ' {
        i -= 1;
    }
    if i >= 2 && &b[i - 2..i] == b"::" {
        Sep::Scope
    } else if i >= 1 && b[i - 1] == b'.' {
        Sep::Dot
    } else {
        Sep::None
    }
}

/// The receiver chain before byte `s`: for `a.b().c::d|` gives
/// `[(None,a), (Dot,b), (Dot,c)]`, each part with the separator before it
/// (the one before `s` is `chain_sep`). Empty when nothing precedes.
pub fn chain_before(text: &str, s: usize) -> Vec<(Sep, String)> {
    let b = text.as_bytes();
    let skip_ws = |mut j: usize| {
        while j > 0 && matches!(b[j - 1], b' ' | b'\t') {
            j -= 1;
        }
        j
    };
    // Parts right to left, each with the separator after it.
    let mut parts: Vec<(String, Sep)> = Vec::new();
    let mut i = s;
    loop {
        let mut j = skip_ws(i);
        let sep = if j >= 2 && &b[j - 2..j] == b"::" {
            j -= 2;
            Sep::Scope
        } else if j >= 1 && b[j - 1] == b'.' {
            j -= 1;
            Sep::Dot
        } else {
            break;
        };
        let mut k = skip_ws(j);
        // Skip a call's parentheses, an index or a parameter list.
        while k > 0 && matches!(b[k - 1], b')' | b']') {
            let (open, close) = if b[k - 1] == b')' {
                (b'(', b')')
            } else {
                (b'[', b']')
            };
            let mut depth = 0;
            let mut m = k;
            while m > 0 {
                m -= 1;
                if b[m] == close {
                    depth += 1;
                } else if b[m] == open {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            k = skip_ws(m);
            if k > 0 && b[k - 1] == b'#' {
                k = skip_ws(k - 1);
            }
        }
        let end = k;
        while k > 0 && is_ident_byte(b[k - 1]) {
            k -= 1;
        }
        if k == end {
            break;
        }
        parts.push((text[k..end].to_string(), sep));
        i = k;
    }
    parts.reverse();
    let mut out = Vec::with_capacity(parts.len());
    let mut before = Sep::None;
    for (name, after) in parts {
        out.push((before, name));
        before = after;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains() {
        let t = "x = a.b().c::d";
        let s = t.len() - 1;
        let c = chain_before(t, s);
        let names: Vec<&str> = c.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        let seps: Vec<Sep> = c.iter().map(|(s, _)| *s).collect();
        assert_eq!(seps, vec![Sep::None, Sep::Dot, Sep::Dot]);
        assert_eq!(chain_sep(t, s), Sep::Scope);
        assert!(chain_before("foo", 0).is_empty());
        let t2 = "p::c::f";
        let c2 = chain_before(t2, 6);
        assert_eq!(
            c2,
            vec![(Sep::None, "p".to_string()), (Sep::Scope, "c".to_string())]
        );
    }
}
