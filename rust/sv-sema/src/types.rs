//! Type strings as slang prints them (`Type::toString`), and the constant
//! evaluation they need: packed and unpacked bounds, parameter values of
//! class specializations and enum values.

use super::*;

/// A constant integer: its width, signedness and value (two-state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CVal {
    pub w: u32,
    pub s: bool,
    pub v: i128,
}

impl CVal {
    pub fn int(v: i128) -> CVal {
        CVal { w: 32, s: true, v }.norm()
    }

    /// Truncated to the width, sign-extended if signed.
    pub fn norm(self) -> CVal {
        let w = self.w.clamp(1, 128);
        let mut v = self.v;
        if w < 128 {
            let mask = (1i128 << w) - 1;
            v &= mask;
            if self.s && (v >> (w - 1)) & 1 == 1 {
                v -= 1i128 << w;
            }
        }
        CVal {
            w: self.w,
            s: self.s,
            v,
        }
    }

    pub fn cast(self, w: u32, s: bool) -> CVal {
        CVal { w, s, v: self.v }.norm()
    }

    fn digits(&self, radix: u32) -> String {
        let mut v = self.v.unsigned_abs();
        if self.s && self.v < 0 {
            v = self.v.unsigned_abs();
        } else if self.v < 0 {
            // Unsigned values wider than i128 holds: shown by their low bits.
            v = self.v as u128;
        }
        if v == 0 {
            return "0".into();
        }
        let mut out = Vec::new();
        while v > 0 {
            out.push(std::char::from_digit((v % radix as u128) as u32, radix).unwrap());
            v /= radix as u128;
        }
        out.iter().rev().collect()
    }

    /// slang's default form: binary below 8 unsigned bits, decimal up to
    /// 32 bits or signed, hex above; a plain number for a signed 32-bit
    /// decimal.
    pub fn show(&self) -> String {
        let (radix, b) = if self.w < 8 && !self.s {
            (2, 'b')
        } else if self.w <= 32 || self.s {
            (10, 'd')
        } else {
            (16, 'h')
        };
        let neg = if self.s && self.v < 0 { "-" } else { "" };
        if radix == 10 && self.w == 32 && self.s {
            return format!("{neg}{}", self.digits(10));
        }
        let s = if self.s { "s" } else { "" };
        format!("{neg}{}'{s}{b}{}", self.w, self.digits(radix))
    }

    /// Always decimal with its size, as enum values print.
    pub fn show_dec(&self) -> String {
        let neg = if self.s && self.v < 0 { "-" } else { "" };
        let s = if self.s { "s" } else { "" };
        format!("{neg}{}'{s}d{}", self.w, self.digits(10))
    }
}

const INT_TYPES: [&str; 9] = [
    "bit", "logic", "reg", "int", "integer", "byte", "shortint", "longint", "time",
];

fn int_width(k: &str) -> (u32, bool) {
    match k {
        "int" | "integer" => (32, true),
        "byte" => (8, true),
        "shortint" => (16, true),
        "longint" => (64, true),
        "time" => (64, false),
        _ => (1, false),
    }
}

impl<'t> Builder<'t> {
    fn toks_of(&self, n: NodeRef) -> Vec<&'t str> {
        self.tokens(n)
            .iter()
            .map(|&t| self.text(n.tree, t))
            .collect()
    }

    /// `pkg::cls::` for a scope (empty for the compilation unit).
    pub(crate) fn scope_path(&self, s: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(s);
        while let Some(c) = cur {
            if self.scopes[c].kind != ScopeKind::Unit {
                parts.push(self.scopes[c].name.clone());
            }
            cur = self.scopes[c].parent;
        }
        parts.reverse();
        parts.iter().map(|p| format!("{p}::")).collect()
    }

    // ---- constants ----------------------------------------------------------

    /// Width and signedness of an integral data type, if it is one.
    pub(crate) fn int_shape(
        &self,
        dt: NodeRef,
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> Option<(u32, bool)> {
        if depth > 16 {
            return None;
        }
        let own = self.own_tokens(dt);
        let kw = own.iter().find(|t| INT_TYPES.contains(t)).copied();
        let implicit = kw.is_none()
            && own
                .iter()
                .all(|t| matches!(*t, "signed" | "unsigned" | "parameter" | "localparam"))
            && (!own.is_empty() || self.child(dt, Dim).is_some());
        if kw.is_none() && !implicit {
            // A typedef of an integral type.
            let toks = self.toks_of(dt);
            if toks.len() == 1 || toks.contains(&"::") {
                if let Some(Found::Typedef(t, s)) = self.lookup_type(dt, scope) {
                    let inner = self.child(t, DataType)?;
                    if self.child(t, Dim).is_some() {
                        return None;
                    }
                    return self.int_shape(inner, s, env, depth + 1);
                }
            }
            if own.first() == Some(&"enum") {
                return Some(match self.child(dt, DataType) {
                    Some(b) => self.int_shape(b, scope, env, depth + 1)?,
                    None => (32, true),
                });
            }
            return None;
        }
        let (mut w, mut s) = kw.map_or((1, false), int_width);
        if own.contains(&"signed") {
            s = true;
        }
        if own.contains(&"unsigned") {
            s = false;
        }
        for d in self.children(dt) {
            if self.kind(d) == Dim {
                let (a, b) = self.dim_range(d, scope, env)?;
                w *= (a - b).unsigned_abs() as u32 + 1;
            }
        }
        Some((w, s))
    }

    fn lookup_type(&self, dt: NodeRef, scope: usize) -> Option<Found> {
        let toks = self.toks_of(dt);
        let path: Vec<&str> = toks
            .iter()
            .take_while(|t| **t != "#" && **t != "[")
            .filter(|t| **t != "::")
            .copied()
            .collect();
        match path.len() {
            0 => None,
            1 => self.lookup(path[0], scope, 0),
            _ => self.lookup_path(&path, scope, 0),
        }
    }

    /// The bounds of a `[a:b]` or `[n]` dimension.
    pub(crate) fn dim_range(&self, d: NodeRef, scope: usize, env: &Env) -> Option<(i128, i128)> {
        let exprs: Vec<NodeRef> = self
            .children(d)
            .into_iter()
            .filter(|c| body_is_expr(self.kind(*c)))
            .collect();
        let own = self.own_tokens(d);
        match exprs.as_slice() {
            [a, b] if own.contains(&":") => Some((
                self.eval(*a, scope, env, 0)?.v,
                self.eval(*b, scope, env, 0)?.v,
            )),
            [n] if !own.contains(&":") => Some((0, self.eval(*n, scope, env, 0)?.v - 1)),
            _ => None,
        }
    }

    /// A named constant seen from `scope`: a parameter or localparam, with
    /// type parameters' and value parameters' bindings from `env`.
    fn const_named(&self, name: &str, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let Some(&(dt, val)) = self.scopes[s].consts.get(name) {
                let v = match (env.get(name), self.scopes[s].kind) {
                    (Some(b), ScopeKind::Class(_)) => {
                        self.eval(b.node, b.scope, &b.env, depth + 1)?
                    }
                    _ => self.eval(val?, s, env, depth + 1)?,
                };
                return Some(
                    match dt.and_then(|d| self.int_shape(d, s, env, depth + 1)) {
                        Some((w, sg)) => v.cast(w, sg),
                        None => v,
                    },
                );
            }
            if let ScopeKind::Class(c) = self.scopes[s].kind {
                // Inherited parameters.
                if let Some((b, be)) = self.base_env(c, env, depth + 1) {
                    if let Some(v) = self.const_named(name, self.classes[b].scope, &be, depth + 1) {
                        return Some(v);
                    }
                }
            } else {
                for (pkg, item) in &self.scopes[s].imports {
                    if item.as_deref().is_some_and(|i| i != name) {
                        continue;
                    }
                    if let Some(&p) = self.packages.get(pkg) {
                        if self.scopes[p].consts.contains_key(name) {
                            return self.const_named(name, p, env, depth + 1);
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        // An enum member.
        self.enum_member(name, scope, depth)
    }

    fn enum_member(&self, name: &str, scope: usize, depth: usize) -> Option<CVal> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            for &t in self.scopes[s].typedefs.values() {
                if let Some(dt) = self.child(t, DataType) {
                    if self.own_tokens(dt).first() == Some(&"enum") {
                        if let Some((_, v)) = self
                            .enum_values(dt, s, &Env::default(), depth + 1)
                            .into_iter()
                            .find(|(n, _)| n == name)
                        {
                            return Some(v);
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// Each enum member with its value.
    pub(crate) fn enum_values(
        &self,
        dt: NodeRef,
        scope: usize,
        env: &Env,
        depth: usize,
    ) -> Vec<(String, CVal)> {
        let mut out = Vec::new();
        if depth > 16 {
            return out;
        }
        let (w, s) = self
            .child(dt, DataType)
            .and_then(|b| self.int_shape(b, scope, env, depth + 1))
            .unwrap_or((32, true));
        let mut next = CVal { w, s, v: 0 };
        for d in self.children(dt) {
            if self.kind(d) != Declarator {
                continue;
            }
            let Some((name, _)) = self.name_of(d) else {
                continue;
            };
            let val = self
                .children(d)
                .into_iter()
                .find(|c| body_is_expr(self.kind(*c)))
                .and_then(|e| self.eval(e, scope, env, depth + 1));
            let v = match val {
                Some(v) => v.cast(w, s),
                None => next,
            };
            // `C[2]` names C0 and C1.
            match self
                .child(d, Dim)
                .and_then(|dim| self.dim_range(dim, scope, env))
            {
                Some((a, b)) => {
                    let mut cur = v;
                    let n = (a - b).abs() + 1;
                    for k in 0..n {
                        out.push((format!("{name}{k}"), cur));
                        cur = CVal {
                            v: cur.v + 1,
                            ..cur
                        }
                        .norm();
                    }
                    next = cur;
                }
                None => {
                    out.push((name, v));
                    next = CVal { v: v.v + 1, ..v }.norm();
                }
            }
        }
        out
    }

    /// The value of a constant expression.
    pub(crate) fn eval(&self, e: NodeRef, scope: usize, env: &Env, depth: usize) -> Option<CVal> {
        if depth > 48 {
            return None;
        }
        let kids: Vec<NodeRef> = self
            .children(e)
            .into_iter()
            .filter(|c| body_is_expr(self.kind(*c)))
            .collect();
        match self.kind(e) {
            Literal => literal(&self.toks_of(e)),
            NameExpr => {
                let toks = self.toks_of(e);
                match toks.as_slice() {
                    [n] => self.const_named(n, scope, env, depth + 1),
                    _ => None,
                }
            }
            ScopedExpr => {
                let toks = self.toks_of(e);
                if toks.len() == 3 && toks[1] == "::" {
                    let p = *self.packages.get(toks[0])?;
                    return self.const_named(toks[2], p, &Env::default(), depth + 1);
                }
                None
            }
            ParenExpr => self.eval(*kids.first()?, scope, env, depth + 1),
            PrefixExpr => {
                let op = *self.own_tokens(e).first()?;
                let a = self.eval(*kids.first()?, scope, env, depth + 1)?;
                Some(match op {
                    "-" => CVal { v: -a.v, ..a }.norm(),
                    "+" => a,
                    "~" => CVal { v: !a.v, ..a }.norm(),
                    "!" => CVal {
                        w: 1,
                        s: false,
                        v: (a.v == 0) as i128,
                    },
                    _ => return None,
                })
            }
            BinaryExpr => {
                let op = *self.own_tokens(e).first()?;
                let a = self.eval(*kids.first()?, scope, env, depth + 1)?;
                let b = self.eval(*kids.get(1)?, scope, env, depth + 1)?;
                binary(op, a, b)
            }
            CondExpr => {
                let c = self.eval(*kids.first()?, scope, env, depth + 1)?;
                let a = self.eval(*kids.get(1)?, scope, env, depth + 1)?;
                let b = self.eval(*kids.get(2)?, scope, env, depth + 1)?;
                let (w, s) = (a.w.max(b.w), a.s && b.s);
                Some(if c.v != 0 { a } else { b }.cast(w, s))
            }
            CallExpr => {
                let callee = *self.children(e).first()?;
                let name = self.toks_of(callee).concat();
                let args: Vec<NodeRef> = self
                    .child(e, ArgList)
                    .map(|al| {
                        self.children(al)
                            .into_iter()
                            .filter(|a| self.kind(*a) == Arg)
                            .filter_map(|a| self.children(a).into_iter().next())
                            .collect()
                    })
                    .unwrap_or_default();
                match name.as_str() {
                    "$clog2" => {
                        let a = self.eval(*args.first()?, scope, env, depth + 1)?;
                        let mut r = 0;
                        while (1i128 << r) < a.v {
                            r += 1;
                        }
                        Some(CVal::int(r))
                    }
                    "$bits" => {
                        let a = *args.first()?;
                        let w = match self.kind(a) {
                            DataType => self.int_shape(a, scope, env, depth + 1)?.0,
                            _ => self.eval(a, scope, env, depth + 1)?.w,
                        };
                        Some(CVal::int(w as i128))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // ---- type strings -------------------------------------------------------

    /// How slang prints a declaration's type: its data type (`None` for an
    /// implicit one) and the unpacked dimensions after the declared name.
    pub(crate) fn type_string(
        &self,
        dt: Option<NodeRef>,
        item: Option<NodeRef>,
        scope: usize,
        env: &Env,
        canonical: bool,
    ) -> String {
        let dims: Vec<NodeRef> = item
            .map(|i| {
                self.children(i)
                    .into_iter()
                    .filter(|c| self.kind(*c) == Dim)
                    .collect()
            })
            .unwrap_or_default();
        if dims.is_empty() {
            return match dt {
                Some(d) => self.ts(d, scope, env, canonical, 0),
                None => "logic".into(),
            };
        }
        // Only the type itself is canonical: an array keeps its element's
        // alias, unless that alias is an unpacked array itself, whose
        // dimensions then follow ours.
        let mut text = self.unpacked(&dims, scope, env);
        let mut cur = dt.map(|d| (d, scope));
        for _ in 0..16 {
            let Some((d, s)) = cur else { break };
            match self.lookup_type(d, s) {
                Some(Found::Typedef(t, ts)) if self.child(t, Dim).is_some() => {
                    let tdims: Vec<NodeRef> = self
                        .children(t)
                        .into_iter()
                        .filter(|c| self.kind(*c) == Dim)
                        .collect();
                    let tenv = self.scope_env(ts, env);
                    text.push_str(&self.unpacked(&tdims, ts, &tenv));
                    cur = self.child(t, DataType).map(|i| (i, ts));
                }
                _ => break,
            }
        }
        let base = match cur {
            Some((d, s)) => self.ts(d, s, env, false, 0),
            None => "logic".into(),
        };
        format!("{base}${text}")
    }

    fn unpacked(&self, dims: &[NodeRef], scope: usize, env: &Env) -> String {
        let mut s = String::new();
        for &d in dims {
            let inner = {
                let t = self.toks_of(d);
                t[1..t.len().saturating_sub(1)].to_vec()
            };
            let text = match inner.as_slice() {
                [] => String::new(),
                ["$"] => "$".into(),
                ["*"] => "*".into(),
                ["$", ":", ..] => match self
                    .children(d)
                    .into_iter()
                    .find(|c| body_is_expr(self.kind(*c)))
                    .and_then(|e| self.eval(e, scope, env, 0))
                {
                    Some(v) => format!("$:{}", v.v),
                    None => inner.concat(),
                },
                _ => {
                    if let Some(t) = self.child(d, DataType) {
                        self.ts(t, scope, env, false, 0)
                    } else if self.dim_shape(d, scope, 0) == TypeShape::AssociativeArray {
                        // `[key_t]` parsed as a name.
                        let e = self.children(d).into_iter().next();
                        e.map_or(inner.concat(), |e| self.ts(e, scope, env, false, 0))
                    } else {
                        match self.dim_range(d, scope, env) {
                            Some((a, b)) => format!("{a}:{b}"),
                            None => inner.concat(),
                        }
                    }
                }
            };
            s.push('[');
            s.push_str(&text);
            s.push(']');
        }
        s
    }

    /// A data type (or a name standing for one) as slang prints it.
    pub(crate) fn ts(
        &self,
        dt: NodeRef,
        scope: usize,
        env: &Env,
        canonical: bool,
        depth: usize,
    ) -> String {
        if depth > 24 {
            return String::new();
        }
        let toks = self.toks_of(dt);
        let own = self.own_tokens(dt);
        let Some(&first) = toks.first() else {
            return "logic".into();
        };
        let kw = own.iter().find(|t| INT_TYPES.contains(t)).copied();
        let implicit = kw.is_none() && matches!(first, "signed" | "unsigned" | "[");
        if kw.is_some() || implicit {
            let k = kw.unwrap_or("logic");
            let default_signed = int_width(k).1;
            let mut s = k.to_string();
            if own.contains(&"signed") && !default_signed {
                s.push_str(" signed");
            }
            if own.contains(&"unsigned") && default_signed {
                s.push_str(" unsigned");
            }
            for d in self.children(dt) {
                if self.kind(d) == Dim {
                    match self.dim_range(d, scope, env) {
                        Some((a, b)) => s.push_str(&format!("[{a}:{b}]")),
                        None => s.push_str(&self.toks_of(d).concat()),
                    }
                }
            }
            return s;
        }
        match first {
            "string" | "event" | "chandle" | "real" | "shortreal" | "realtime" | "void" => {
                return first.to_string();
            }
            "virtual" => {
                let names: Vec<&str> = toks
                    .iter()
                    .skip(1)
                    .filter(|t| **t != "interface")
                    .take_while(|t| **t != "#")
                    .copied()
                    .collect();
                let name = names.first().copied().unwrap_or("");
                let params = self.interface_params(name, dt, scope, env);
                return format!("{name}{params}{}", names[1.min(names.len())..].concat());
            }
            "enum" => return self.enum_string(dt, scope, env, None),
            "struct" | "union" => return self.struct_string(dt, scope, env, None),
            _ => {}
        }
        let path: Vec<&str> = toks
            .iter()
            .take_while(|t| **t != "#" && **t != "[")
            .filter(|t| **t != "::")
            .copied()
            .collect();
        // A type parameter bound by the specialization.
        if path.len() == 1 {
            if let Some(b) = env.get(first) {
                if canonical {
                    return self.ts(b.node, b.scope, &b.env, true, depth + 1);
                }
            }
        }
        let found = match path.len() {
            0 => None,
            1 => self.lookup(path[0], scope, depth + 1),
            _ => self.lookup_path(&path, scope, depth + 1),
        };
        match found {
            Some(Found::Class(c)) => {
                let args = self.bind_args(c, dt, scope, env);
                self.class_string(c, &args, depth + 1)
            }
            Some(Found::Typedef(t, s)) => {
                let name = self.name_of(t).map(|n| n.0).unwrap_or_default();
                if !canonical {
                    return format!("{}{name}", self.scope_path(s));
                }
                let tenv = self.scope_env(s, env);
                let inner = self.child(t, DataType);
                let dims: Vec<NodeRef> = self
                    .children(t)
                    .into_iter()
                    .filter(|c| self.kind(*c) == Dim)
                    .collect();
                let alias = format!("{}{name}", self.scope_path(s));
                let base = match inner {
                    Some(i) if dims.is_empty() => match self.own_tokens(i).first().copied() {
                        Some("enum") => self.enum_string(i, s, &tenv, Some(&alias)),
                        Some("struct" | "union") => self.struct_string(i, s, &tenv, Some(&alias)),
                        _ => self.ts(i, s, &tenv, true, depth + 1),
                    },
                    Some(i) => self.ts(i, s, &tenv, false, depth + 1),
                    None => "logic".into(),
                };
                if dims.is_empty() {
                    base
                } else {
                    format!("{base}${}", self.unpacked(&dims, s, &tenv))
                }
            }
            Some(Found::Param(default, s)) => {
                let name = path[path.len() - 1];
                if !canonical {
                    return format!("{}{name}", self.scope_path(s));
                }
                match env.get(name) {
                    Some(b) => self.ts(b.node, b.scope, &b.env, true, depth + 1),
                    None => default.map_or(String::new(), |d| {
                        let p = self.scopes[s].parent.unwrap_or(s);
                        self.ts(d, p, &Env::default(), true, depth + 1)
                    }),
                }
            }
            None => {
                let name = if path.len() == 2 && path[0] == "std" {
                    path[1]
                } else {
                    path[0]
                };
                match name {
                    "mailbox" => {
                        let arg = self
                            .child(dt, ParamList)
                            .and_then(|pl| self.child(pl, ArgList))
                            .and_then(|al| {
                                self.children(al).into_iter().find(|a| self.kind(*a) == Arg)
                            })
                            .and_then(|a| self.children(a).into_iter().next());
                        match arg {
                            Some(a) => {
                                format!("mailbox#({})", self.ts(a, scope, env, false, depth + 1))
                            }
                            None => "mailbox#(untyped)".into(),
                        }
                    }
                    "semaphore" | "process" => name.to_string(),
                    _ if self.undeclared_type(dt) => "<error>".into(),
                    _ => self.tree(dt).bare_text(dt.node),
                }
            }
        }
    }

    /// `#(W=8)` for an interface with parameters: the vif's arguments or
    /// the defaults.
    fn interface_params(&self, name: &str, dt: NodeRef, scope: usize, env: &Env) -> String {
        let Some(&decl) = self.interfaces.get(name) else {
            return String::new();
        };
        let Some(pl) = self
            .child(decl, Header)
            .and_then(|h| self.child(h, ParamList))
        else {
            return String::new();
        };
        let args: Vec<NodeRef> = self
            .child(dt, ParamList)
            .and_then(|p| self.child(p, ArgList))
            .map(|al| {
                self.children(al)
                    .into_iter()
                    .filter(|a| self.kind(*a) == Arg)
                    .collect()
            })
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut pos = 0;
        for p in self.children(pl) {
            if self.kind(p) != ParamDecl || self.own_tokens(p).contains(&"type") {
                continue;
            }
            let Some(d) = self.child(p, Declarator) else {
                continue;
            };
            let Some((pname, _)) = self.name_of(d) else {
                continue;
            };
            let default = self
                .children(d)
                .into_iter()
                .find(|c| body_is_expr(self.kind(*c)));
            let actual = args
                .iter()
                .find(|a| self.own_tokens(**a).get(1) == Some(&pname.as_str()))
                .or_else(|| {
                    args.get(pos)
                        .filter(|a| self.own_tokens(**a).first() != Some(&"."))
                })
                .and_then(|a| {
                    self.children(*a)
                        .into_iter()
                        .find(|c| body_is_expr(self.kind(*c)))
                });
            pos += 1;
            let v = match actual {
                Some(a) => self.eval(a, scope, env, 0),
                None => default.and_then(|e| self.eval(e, scope, &Env::default(), 0)),
            };
            let v = match (
                v,
                self.child(p, DataType)
                    .and_then(|t| self.int_shape(t, scope, env, 0)),
            ) {
                (Some(v), Some((w, s))) => Some(v.cast(w, s)),
                (v, _) => v,
            };
            out.push(format!("{pname}={}", v.map_or(String::new(), |v| v.show())));
        }
        if out.is_empty() {
            String::new()
        } else {
            format!("#({})", out.join(","))
        }
    }

    /// The environment typedefs of class scope `s` are read in: the one
    /// given if it is that class's, else its representative.
    fn scope_env(&self, s: usize, env: &Env) -> Env {
        match self.scopes[s].kind {
            ScopeKind::Class(c) => {
                if env.is_empty() {
                    self.rep_env.get(&c).cloned().unwrap_or_default()
                } else {
                    env.clone()
                }
            }
            _ => Env::default(),
        }
    }

    /// `name#(args)` for a specialization, `name` otherwise.
    pub(crate) fn class_string(&self, c: usize, env: &Env, depth: usize) -> String {
        let def = &self.classes[c];
        let mut s = def.name.clone();
        if !def.generic || depth > 24 {
            return s;
        }
        s.push_str("#(");
        for (k, (name, is_type, default)) in def.params.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            if *is_type {
                let printed = match env.get(name) {
                    Some(b) => {
                        // `C#(T)` inside C itself names the specialization
                        // being printed: its T stands for what T is bound to.
                        let mut b = b.clone();
                        if b.scope == def.scope {
                            let toks = self.toks_of(b.node);
                            if let [n] = toks.as_slice() {
                                if let Some(next) = b.env.get(*n).cloned() {
                                    b = next;
                                }
                            }
                        }
                        self.ts(b.node, b.scope, &b.env, false, depth + 1)
                    }
                    None => default.map_or(String::new(), |d| {
                        self.ts(d, def.scope, env, false, depth + 1)
                    }),
                };
                s.push_str(&printed);
            } else {
                let v = self.const_named(name, def.scope, env, depth + 1);
                match v {
                    Some(v) => s.push_str(&v.show()),
                    None => {
                        if let Some(d) = default {
                            s.push_str(&self.tree(*d).bare_text(d.node));
                        }
                    }
                }
            }
        }
        s.push(')');
        s
    }

    fn enum_string(&self, dt: NodeRef, scope: usize, env: &Env, alias: Option<&str>) -> String {
        let vals = self.enum_values(dt, scope, env, 0);
        let body: Vec<String> = vals
            .iter()
            .map(|(n, v)| format!("{n}={}", v.show_dec()))
            .collect();
        let name = match alias {
            Some(a) => a.to_string(),
            None => format!("{}e$1", self.scope_path(scope)),
        };
        format!("enum{{{}}}{name}", body.join(","))
    }

    fn struct_string(&self, dt: NodeRef, scope: usize, env: &Env, _alias: Option<&str>) -> String {
        let own = self.own_tokens(dt);
        let kind = own[0];
        let mut s = kind.to_string();
        if own.contains(&"packed") {
            s.push_str(" packed");
        }
        s.push('{');
        for m in self.children(dt) {
            if self.kind(m) != DataDecl {
                continue;
            }
            let mdt = self.child(m, DataType);
            for d in self.children(m) {
                if self.kind(d) == Declarator {
                    if let Some((n, _)) = self.name_of(d) {
                        let t = self.type_string(mdt, Some(d), scope, env, false);
                        s.push_str(&format!("{t} {n};"));
                    }
                }
            }
        }
        s.push('}');
        // slang names anonymous structs `s$N` / `u$N` in their scope, by
        // the order it created them.
        let letter = if kind == "union" { 'u' } else { 's' };
        let n = self.anon_index(dt, scope, kind);
        format!("{s}{}{letter}${n}", self.scope_path(scope))
    }

    /// 1-based position of a struct or union type among those of the same
    /// kind declared directly in its scope's typedefs, in source order.
    fn anon_index(&self, dt: NodeRef, scope: usize, kind: &str) -> usize {
        let mut all: Vec<NodeRef> = self.scopes[scope]
            .typedefs
            .values()
            .filter_map(|&t| self.child(t, DataType))
            .filter(|d| self.own_tokens(*d).first() == Some(&kind))
            .collect();
        all.sort_by_key(|d| (d.tree, self.tokens(*d).first().copied().unwrap_or(0)));
        all.iter().position(|d| *d == dt).map_or(1, |p| p + 1)
    }
}

fn body_is_expr(k: SyntaxKind) -> bool {
    matches!(
        k,
        BinaryExpr
            | PrefixExpr
            | PostfixExpr
            | CondExpr
            | ParenExpr
            | Literal
            | NameExpr
            | ScopedExpr
            | MemberExpr
            | SelectExpr
            | CallExpr
            | ConcatExpr
            | CastExpr
    )
}

/// `4'hA`, `12`, `'1`, `2'sb11` as a value.
fn literal(toks: &[&str]) -> Option<CVal> {
    let joined: String = toks.concat().replace('_', "");
    if let Some(i) = joined.find('\'') {
        let (size, rest) = joined.split_at(i);
        let rest = &rest[1..];
        let (signed, rest) = match rest.strip_prefix(['s', 'S']) {
            Some(r) => (true, r),
            None => (false, rest),
        };
        let mut chars = rest.chars();
        let b = chars.next()?.to_ascii_lowercase();
        let digits: String = chars.collect();
        let radix = match b {
            'b' => 2,
            'o' => 8,
            'd' => 10,
            'h' => 16,
            '0' | '1' if size.is_empty() => {
                // `'0` / `'1`: all bits, here taken as one.
                return Some(CVal {
                    w: 1,
                    s: false,
                    v: (b == '1') as i128,
                });
            }
            _ => return None,
        };
        let v = i128::from_str_radix(&digits.to_ascii_lowercase(), radix).ok()?;
        let w = if size.is_empty() {
            32
        } else {
            size.parse().ok()?
        };
        return Some(CVal { w, s: signed, v }.norm());
    }
    joined.parse::<i128>().ok().map(CVal::int)
}

fn binary(op: &str, a: CVal, b: CVal) -> Option<CVal> {
    let w = a.w.max(b.w);
    let s = a.s && b.s;
    let bool_ = |x: bool| CVal {
        w: 1,
        s: false,
        v: x as i128,
    };
    let (x, y) = (a.cast(w, s).v, b.cast(w, s).v);
    let r = match op {
        "+" => x + y,
        "-" => x - y,
        "*" => x * y,
        "/" => x.checked_div(y)?,
        "%" => x.checked_rem(y)?,
        "**" => x.checked_pow(u32::try_from(y).ok()?)?,
        "&" => x & y,
        "|" => x | y,
        "^" => x ^ y,
        "<<" | "<<<" => {
            return Some(
                CVal {
                    v: a.v << b.v.clamp(0, 127),
                    ..a
                }
                .norm(),
            )
        }
        ">>" | ">>>" => {
            return Some(
                CVal {
                    v: a.v >> b.v.clamp(0, 127),
                    ..a
                }
                .norm(),
            )
        }
        "==" | "===" => return Some(bool_(x == y)),
        "!=" | "!==" => return Some(bool_(x != y)),
        "<" => return Some(bool_(x < y)),
        "<=" => return Some(bool_(x <= y)),
        ">" => return Some(bool_(x > y)),
        ">=" => return Some(bool_(x >= y)),
        "&&" => return Some(bool_(x != 0 && y != 0)),
        "||" => return Some(bool_(x != 0 || y != 0)),
        _ => return None,
    };
    Some(CVal { w, s, v: r }.norm())
}
