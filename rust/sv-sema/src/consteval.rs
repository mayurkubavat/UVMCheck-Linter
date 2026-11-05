//! Constant evaluation beyond expressions (builder phase N3): constant
//! function calls (IEEE 1800 13.4.3), concatenation and replication, and
//! elements of constant unpacked arrays (`'{1, 2, 3}`).
//!
//! A constant function runs on a small interpreter: its arguments and
//! locals live in a frame on `Builder::frames`, which name lookups in
//! `eval` read first. Statements: declarations, assignments (`=`, `op=`,
//! `++`, `--`), `if`/`else`, blocks, `for`, `while`, `repeat` and
//! `return`. Anything else gives up (no value), never a wrong one.

use super::types::CVal;
use super::*;

/// How a statement ended.
enum Flow {
    Next,
    Return(Option<CVal>),
}

/// Loop iterations allowed in one call, so a runaway loop gives up.
const MAX_STEPS: usize = 100_000;

impl Builder<'_> {
    /// A user function called in a constant expression.
    pub(crate) fn call_const(
        &self,
        name: &str,
        args: &[NodeRef],
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> Option<CVal> {
        if depth > 32 || self.frames.borrow().len() > 16 {
            return None;
        }
        let decl = self.function_decl(name, scope)?;
        if self.kind(decl) != FunctionDecl {
            return None;
        }
        // Arguments are evaluated where the call is.
        let vals: Vec<CVal> = args.iter().map(|&a| self.eval(a, scope, env, depth + 1)).collect::<Option<_>>()?;
        let fscope = self.decl_scope(decl).unwrap_or(scope);
        let mut frame: HashMap<String, CVal> = HashMap::new();
        let mut k = 0;
        let mut ports: Vec<(String, Option<NodeRef>)> = Vec::new();
        if let Some(pl) = self.child(decl, PortList) {
            let mut dt = None;
            for p in self.children(pl) {
                if self.kind(p) != Port {
                    continue;
                }
                if let Some(d) = self.child(p, DataType) {
                    dt = Some(d);
                }
                if let Some((n, _)) = self.child(p, Declarator).and_then(|d| self.name_of(d)) {
                    ports.push((n, dt));
                }
            }
        }
        let body = self.child(decl, Body)?;
        // Old-style `input int a;` in the body.
        for d in self.children(body) {
            if self.kind(d) == DataDecl
                && self.own_tokens(d).first().is_some_and(|t| matches!(*t, "input" | "inout" | "output" | "ref"))
            {
                let dt = self.child(d, DataType);
                for x in self.children(d) {
                    if let Some((n, _)) = (self.kind(x) == Declarator).then(|| self.name_of(x)).flatten() {
                        ports.push((n, dt));
                    }
                }
            }
        }
        for (n, dt) in ports {
            let v = *vals.get(k)?;
            k += 1;
            frame.insert(n, self.fit(v, dt, fscope, env));
        }
        let ret_dt = self.child(decl, ReturnType).and_then(|r| self.child(r, DataType));
        let (fname, _) = self.name_of(decl)?;
        frame.insert(fname.clone(), self.fit(CVal::int(0), ret_dt, fscope, env));
        self.frames.borrow_mut().push(frame);
        let mut steps = 0;
        let flow = self.exec_list(&self.children(body), fscope, env, depth + 1, &mut steps);
        let frame = self.frames.borrow_mut().pop()?;
        let v = match flow? {
            Flow::Return(Some(v)) => v,
            _ => *frame.get(&fname)?,
        };
        Some(self.fit(v, ret_dt, fscope, env))
    }

    /// A value cast to a declared integral type (unchanged when not one).
    fn fit(&self, v: CVal, dt: Option<NodeRef>, scope: usize, env: &Env) -> CVal {
        match dt.and_then(|d| self.int_shape(d, scope, env, 0)) {
            Some((w, s)) => v.cast(w, s),
            None if dt.is_none() => v,
            None => v,
        }
    }

    /// The scope a declaration sits in.
    fn decl_scope(&self, decl: NodeRef) -> Option<usize> {
        let t = self.tree(decl);
        let parent = {
            let mut p = None;
            for n in t.preorder() {
                if t.children(n).any(|c| c == decl.node) {
                    p = Some(n);
                    break;
                }
            }
            p?
        };
        let pr = NodeRef { tree: decl.tree, node: parent };
        if parent == t.root {
            return Some(self.unit);
        }
        self.node_scope.get(&pr).copied()
    }

    fn function_decl(&self, name: &str, scope: usize) -> Option<NodeRef> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let Some(Member::Sub { decl }) = self.scopes[s].values.get(name) {
                return Some(*decl);
            }
            for (pkg, item) in &self.scopes[s].imports {
                if item.as_deref().is_some_and(|i| i != name) {
                    continue;
                }
                if let Some(&p) = self.packages.get(pkg) {
                    if let Some(Member::Sub { decl }) = self.scopes[p].values.get(name) {
                        return Some(*decl);
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    fn exec_list(&self, stmts: &[NodeRef], scope: usize, env: &Env, depth: usize, steps: &mut usize) -> Option<Flow> {
        for &st in stmts {
            match self.exec(st, scope, env, depth, steps)? {
                Flow::Next => {}
                r => return Some(r),
            }
        }
        Some(Flow::Next)
    }

    fn set_local(&self, name: &str, v: CVal) -> Option<()> {
        let mut frames = self.frames.borrow_mut();
        let f = frames.last_mut()?;
        let old = f.get_mut(name)?;
        *old = v.cast(old.w, old.s);
        Some(())
    }

    pub(crate) fn get_local(&self, name: &str) -> Option<CVal> {
        self.frames.borrow().last()?.get(name).copied()
    }

    fn exec(&self, st: NodeRef, scope: usize, env: &Env, depth: usize, steps: &mut usize) -> Option<Flow> {
        *steps += 1;
        if *steps > MAX_STEPS {
            return None;
        }
        match self.kind(st) {
            DataDecl => {
                let own = self.own_tokens(st);
                if own.first().is_some_and(|t| matches!(*t, "input" | "inout" | "output" | "ref")) {
                    return Some(Flow::Next);
                }
                let dt = self.child(st, DataType);
                for d in self.children(st) {
                    if self.kind(d) != Declarator {
                        continue;
                    }
                    let (n, _) = self.name_of(d)?;
                    let init = self.children(d).into_iter().find(|c| !matches!(self.kind(*c), Name | Dim));
                    let v = match init {
                        Some(e) => self.eval(e, scope, env, depth + 1)?,
                        None => CVal::int(0),
                    };
                    let v = self.fit(v, dt, scope, env);
                    self.frames.borrow_mut().last_mut()?.insert(n, v);
                }
                Some(Flow::Next)
            }
            ExprStmt => {
                let e = *self.children(st).first()?;
                self.effect(e, scope, env, depth)?;
                Some(Flow::Next)
            }
            ReturnStmt => match self.children(st).first() {
                Some(&e) => Some(Flow::Return(Some(self.eval(e, scope, env, depth + 1)?))),
                None => Some(Flow::Return(None)),
            },
            SeqBlock => self.exec_list(&self.children(st), scope, env, depth, steps),
            EmptyStmt => Some(Flow::Next),
            IfStmt => {
                let kids = self.children(st);
                let cond = self.eval(*kids.first()?, scope, env, depth + 1)?;
                if cond.v != 0 {
                    self.exec(*kids.get(1)?, scope, env, depth, steps)
                } else {
                    match kids.iter().find(|k| self.kind(**k) == ElseClause) {
                        Some(&e) => self.exec(*self.children(e).first()?, scope, env, depth, steps),
                        None => Some(Flow::Next),
                    }
                }
            }
            LoopStmt => self.exec_loop(st, scope, env, depth, steps),
            _ => None,
        }
    }

    /// An expression evaluated for its effect: an assignment or `i++`.
    fn effect(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<()> {
        let kids = self.children(e);
        match self.kind(e) {
            BinaryExpr => {
                let op = *self.own_tokens(e).first()?;
                let lhs = *kids.first()?;
                let rhs = *kids.get(1)?;
                if self.kind(lhs) != NameExpr {
                    return None;
                }
                let name = self.toks_of(lhs).concat();
                let r = self.eval(rhs, scope, env, depth + 1)?;
                let v = if op == "=" {
                    r
                } else {
                    let cur = self.get_local(&name)?;
                    let bop = op.strip_suffix('=')?;
                    types::binary_op(bop, cur, r)?
                };
                self.set_local(&name, v)
            }
            PostfixExpr | PrefixExpr => {
                let op = *self.own_tokens(e).first()?;
                let x = *kids.first()?;
                let name = self.toks_of(x).concat();
                let cur = self.get_local(&name)?;
                let d = match op {
                    "++" => 1,
                    "--" => -1,
                    _ => return None,
                };
                self.set_local(&name, CVal { v: cur.v + d, ..cur }.norm())
            }
            _ => None,
        }
    }

    fn exec_loop(&self, st: NodeRef, scope: usize, env: &Env, depth: usize, steps: &mut usize) -> Option<Flow> {
        let t = self.tree(st);
        let kw = *self.own_tokens(st).first()?;
        // The parts of `for (init; cond; step) body` split by its own tokens.
        let mut part = 0;
        let mut parts: [Vec<NodeRef>; 4] = Default::default();
        for c in &t.node(st.node).children {
            match *c {
                sv_syntax::Child::Token(i) => match &*t.tokens[i as usize].text {
                    ";" | ")" => part += 1,
                    _ => {}
                },
                sv_syntax::Child::Node(n) => parts[part.min(3)].push(NodeRef { tree: st.tree, node: n }),
            }
        }
        match kw {
            "for" => {
                // Loop variables get their own frame entries (and stay; harmless).
                for &i in &parts[0] {
                    match self.kind(i) {
                        DataDecl => {
                            self.exec(i, scope, env, depth, steps)?;
                        }
                        _ => self.effect(i, scope, env, depth)?,
                    }
                }
                let body = *parts[3].first()?;
                loop {
                    *steps += 1;
                    if *steps > MAX_STEPS {
                        return None;
                    }
                    if let Some(&c) = parts[1].first() {
                        if self.eval(c, scope, env, depth + 1)?.v == 0 {
                            break;
                        }
                    }
                    if let r @ Flow::Return(_) = self.exec(body, scope, env, depth, steps)? {
                        return Some(r);
                    }
                    for &s in &parts[2] {
                        self.effect(s, scope, env, depth)?;
                    }
                }
                Some(Flow::Next)
            }
            "while" => {
                let cond = *parts[0].first()?;
                let body = *parts[1].first()?;
                while self.eval(cond, scope, env, depth + 1)?.v != 0 {
                    *steps += 1;
                    if *steps > MAX_STEPS {
                        return None;
                    }
                    if let r @ Flow::Return(_) = self.exec(body, scope, env, depth, steps)? {
                        return Some(r);
                    }
                }
                Some(Flow::Next)
            }
            "repeat" => {
                let n = self.eval(*parts[0].first()?, scope, env, depth + 1)?.v;
                let body = *parts[1].first()?;
                for _ in 0..n.clamp(0, MAX_STEPS as i128) {
                    if let r @ Flow::Return(_) = self.exec(body, scope, env, depth, steps)? {
                        return Some(r);
                    }
                }
                Some(Flow::Next)
            }
            _ => None,
        }
    }

    /// `{a, b}` and `{n{a, b}}`.
    pub(crate) fn eval_concat(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let kids = self.children(e);
        let own = self.own_tokens(e);
        if kids.len() == 2 && !own.contains(&",") && self.kind(kids[1]) == ConcatExpr {
            let n = self.eval(kids[0], scope, env, depth + 1)?.v;
            let inner = self.eval_concat(kids[1], scope, env, depth + 1)?;
            if !(0..=128).contains(&n) || inner.w as i128 * n > 128 {
                return None;
            }
            let mut v: i128 = 0;
            for _ in 0..n {
                v = (v << inner.w) | (inner.v & mask(inner.w));
            }
            return Some(CVal { w: (inner.w as i128 * n).max(1) as u32, s: false, v }.norm());
        }
        let mut w = 0u32;
        let mut v: i128 = 0;
        for k in kids {
            let x = self.eval(k, scope, env, depth + 1)?;
            w += x.w;
            if w > 128 {
                return None;
            }
            v = (v << x.w) | (x.v & mask(x.w));
        }
        Some(CVal { w: w.max(1), s: false, v }.norm())
    }

    /// `arr[i]` of a constant unpacked array parameter (`'{...}`).
    pub(crate) fn eval_select(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let kids = self.children(e);
        let base = *kids.first()?;
        let idx = self.eval(*kids.get(1)?, scope, env, depth + 1)?.v;
        let toks = self.toks_of(base);
        let (name, mut s) = match toks.as_slice() {
            [n] => (*n, scope),
            [p, "::", n] => (*n, *self.packages.get(*p)?),
            _ => return None,
        };
        // The constant's declaration, up the scopes.
        let (dt, val, decl_scope) = loop {
            if let Some(&(dt, val)) = self.scopes[s].consts.get(name) {
                break (dt, val?, s);
            }
            s = self.scopes[s].parent?;
        };
        if self.kind(val) != AssignPattern {
            // A bit-select of an integral constant.
            let v = self.eval(base, scope, env, depth + 1)?;
            if idx < 0 || idx >= v.w as i128 {
                return None;
            }
            return Some(CVal { w: 1, s: false, v: (v.v >> idx) & 1 });
        }
        let elems: Vec<NodeRef> = self.children(val).into_iter().filter(|c| types::is_expr(self.kind(*c))).collect();
        // The declarator's first unpacked dimension decides the order.
        let decl = self.children(self.decl_of_const(name, decl_scope)?).into_iter().find(|d| self.kind(*d) == Dim);
        let pos = match decl.and_then(|d| self.dim_range(d, decl_scope, env)) {
            Some((l, r)) if self.children(decl?).into_iter().filter(|c| types::is_expr(self.kind(*c))).count() == 2 => {
                let p = if l >= r { l - idx } else { idx - l };
                p
            }
            _ => idx,
        };
        let el = *elems.get(usize::try_from(pos).ok()?)?;
        let v = self.eval(el, decl_scope, env, depth + 1)?;
        Some(self.fit(v, dt, decl_scope, env))
    }

    /// The Declarator of a constant in a scope.
    fn decl_of_const(&self, name: &str, s: usize) -> Option<NodeRef> {
        let (_, val) = *self.scopes[s].consts.get(name)?;
        let val = val?;
        let t = self.tree(val);
        let mut found = None;
        for n in t.preorder() {
            if t.children(n).any(|c| c == val.node) {
                found = Some(NodeRef { tree: val.tree, node: n });
                break;
            }
        }
        found
    }
}

fn mask(w: u32) -> i128 {
    if w >= 127 {
        -1
    } else {
        (1i128 << w) - 1
    }
}
