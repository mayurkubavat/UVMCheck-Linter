//! Per-file index: the declarations a file holds (with their scopes, types
//! and doc comments) and every identifier it uses, built from the sv-syntax
//! tree. Includes are not followed: each file is indexed on its own, so an
//! included class file holds its own symbols and positions stay in it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sv_syntax::{Child, NodeId, SyntaxKind as K, TokenKind, Tree};

use crate::text::LineIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymKind {
    Module,
    Interface,
    Program,
    Package,
    Checker,
    Class,
    Function,
    Task,
    Typedef,
    EnumMember,
    Constraint,
    Covergroup,
    Property,
    Sequence,
    Clocking,
    Parameter,
    Variable,
    Port,
    Instance,
    Modport,
    Genvar,
    Macro,
}

impl SymKind {
    /// Holds members other files can reach by name (`pkg::x`, `obj.x`).
    pub fn is_scope(self) -> bool {
        matches!(
            self,
            SymKind::Module
                | SymKind::Interface
                | SymKind::Program
                | SymKind::Package
                | SymKind::Checker
                | SymKind::Class
        )
    }

    pub fn is_type(self) -> bool {
        matches!(
            self,
            SymKind::Module
                | SymKind::Interface
                | SymKind::Program
                | SymKind::Package
                | SymKind::Class
                | SymKind::Typedef
                | SymKind::Checker
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            SymKind::Module => "module",
            SymKind::Interface => "interface",
            SymKind::Program => "program",
            SymKind::Package => "package",
            SymKind::Checker => "checker",
            SymKind::Class => "class",
            SymKind::Function => "function",
            SymKind::Task => "task",
            SymKind::Typedef => "typedef",
            SymKind::EnumMember => "enum value",
            SymKind::Constraint => "constraint",
            SymKind::Covergroup => "covergroup",
            SymKind::Property => "property",
            SymKind::Sequence => "sequence",
            SymKind::Clocking => "clocking",
            SymKind::Parameter => "parameter",
            SymKind::Variable => "variable",
            SymKind::Port => "port",
            SymKind::Instance => "instance",
            SymKind::Modport => "modport",
            SymKind::Genvar => "genvar",
            SymKind::Macro => "macro",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymKind,
    /// Byte range of the whole declaration.
    pub start: u32,
    pub end: u32,
    /// Byte range of the name.
    pub sel_start: u32,
    pub sel_end: u32,
    pub parent: Option<u32>,
    /// The declaration's first line(s), whitespace collapsed.
    pub detail: String,
    /// Variables, ports, parameters, instances: the declared type's name;
    /// classes: the base class; functions: the return type.
    pub type_name: Option<String>,
    /// `function void C::f()` written outside its class: `C`.
    pub scope_class: Option<String>,
    /// Inside a function, task or block body (or a method's port list).
    pub local: bool,
    /// The comment right above the declaration, or after it on its line.
    pub doc: String,
}

#[derive(Debug, Default)]
pub struct FileIndex {
    pub path: PathBuf,
    pub text: String,
    pub lines: LineIndex,
    pub symbols: Vec<Symbol>,
    /// Identifier -> byte offsets of its uses (macros as `` `name``).
    pub idents: HashMap<String, Vec<u32>>,
    /// Names of packages imported with `import p::*` or `import p::x`.
    pub imports: Vec<String>,
}

impl FileIndex {
    pub fn build(path: &Path, text: String) -> FileIndex {
        // max_include_depth 1: the file alone, no `include followed.
        let opts = sv_syntax::Options { max_include_depth: 1, ..Default::default() };
        let tree = sv_syntax::parse_source(&text, &path.to_string_lossy(), opts);
        let mut b = Builder { tree: &tree, text: &text, syms: Vec::new(), imports: Vec::new() };
        b.walk(tree.root, None, false);
        let mut idents: HashMap<String, Vec<u32>> = HashMap::new();
        for t in &tree.tokens {
            if t.from_macro || t.origin.file != 0 {
                continue;
            }
            if matches!(t.kind, TokenKind::Identifier | TokenKind::EscapedIdentifier) {
                idents.entry(t.text.to_string()).or_default().push(t.origin.offset);
            }
        }
        for m in &tree.macro_uses {
            if m.origin.file == 0 {
                idents.entry(format!("`{}", m.name)).or_default().push(m.origin.offset);
            }
        }
        let Builder { mut syms, imports, .. } = b;
        macro_defs(&text, &mut syms, &mut idents);
        FileIndex { path: path.to_path_buf(), lines: LineIndex::new(&text), text, symbols: syms, idents, imports }
    }

    pub fn position(&self, offset: u32) -> (u32, u32) {
        self.lines.position(&self.text, offset)
    }

    pub fn offset(&self, line: u32, col: u32) -> u32 {
        self.lines.offset(&self.text, line, col)
    }

    pub fn children(&self, parent: Option<u32>) -> impl Iterator<Item = (u32, &Symbol)> {
        self.symbols.iter().enumerate().filter(move |(_, s)| s.parent == parent).map(|(i, s)| (i as u32, s))
    }

    /// Symbols whose declaration contains `offset`, outermost first.
    pub fn enclosing(&self, offset: u32) -> Vec<u32> {
        let mut out: Vec<u32> = (0..self.symbols.len() as u32)
            .filter(|&i| {
                let s = &self.symbols[i as usize];
                s.start <= offset
                    && offset <= s.end
                    && (s.kind.is_scope()
                        || matches!(
                            s.kind,
                            SymKind::Function | SymKind::Task | SymKind::Covergroup | SymKind::Clocking
                        ))
            })
            .collect();
        out.sort_by_key(|&i| (self.symbols[i as usize].start, std::cmp::Reverse(self.symbols[i as usize].end)));
        out
    }
}

struct Builder<'a> {
    tree: &'a Tree,
    text: &'a str,
    syms: Vec<Symbol>,
    imports: Vec<String>,
}

impl Builder<'_> {
    /// Byte range of a node in this file, if both ends are written here.
    fn range(&self, n: NodeId) -> Option<(u32, u32)> {
        let (f, l) = self.tree.token_bounds(n)?;
        let ok = |t: u32| {
            let tok = &self.tree.tokens[t as usize];
            !tok.from_macro && tok.origin.file == 0
        };
        // Shrink to tokens written here (a macro may end a declaration).
        let toks = self.tree.token_range(n);
        let first = if ok(f) { f } else { *toks.iter().find(|&&t| ok(t))? };
        let last = if ok(l) { l } else { *toks.iter().rev().find(|&&t| ok(t))? };
        let a = &self.tree.tokens[first as usize];
        let b = &self.tree.tokens[last as usize];
        Some((a.origin.offset, b.origin.offset + b.text.len() as u32))
    }

    /// The last identifier of a Name node and what came before `::`.
    fn name_of(&self, name: NodeId) -> Option<(String, Option<String>, u32, u32)> {
        let toks = self.tree.token_range(name);
        let idx = *toks.iter().rev().find(|&&t| {
            let k = self.tree.tokens[t as usize].kind;
            matches!(k, TokenKind::Identifier | TokenKind::EscapedIdentifier | TokenKind::SystemIdentifier)
                || self.tree.tokens[t as usize].text.as_ref() == "new"
        })?;
        let tok = &self.tree.tokens[idx as usize];
        if tok.from_macro || tok.origin.file != 0 {
            return None;
        }
        let mut scope = None;
        let pos = toks.iter().position(|&t| t == idx).unwrap_or(0);
        if pos >= 2 && self.tree.tokens[toks[pos - 1] as usize].text.as_ref() == "::" {
            scope = Some(self.tree.tokens[toks[pos - 2] as usize].text.to_string());
        }
        Some((tok.text.to_string(), scope, tok.origin.offset, tok.origin.offset + tok.text.len() as u32))
    }

    /// The declaration text up to its first `;` (or the whole node),
    /// whitespace collapsed and comments dropped.
    fn detail(&self, n: NodeId, stop_at_semicolon: bool) -> String {
        let toks = self.tree.token_range(n);
        join_tokens(
            toks.iter().map(|&t| &self.tree.tokens[t as usize]).filter(|t| !t.from_macro).map(|t| t.text.as_ref()),
            stop_at_semicolon,
        )
    }

    /// The first identifier of a DataType node (`my_item` in
    /// `virtual my_if`, `uvm_driver` in `uvm_driver #(pkt)`).
    fn type_name(&self, dt: NodeId) -> Option<String> {
        let tree = self.tree;
        let mut out: Option<String> = None;
        for c in &tree.node(dt).children {
            match *c {
                Child::Token(t) => {
                    let tok = &tree.tokens[t as usize];
                    match tok.kind {
                        TokenKind::Identifier | TokenKind::EscapedIdentifier => {
                            // `pkg::cls` keeps the last part.
                            out = Some(tok.text.to_string());
                        }
                        TokenKind::Keyword
                            if out.is_none()
                                && !matches!(
                                    tok.text.as_ref(),
                                    "virtual"
                                        | "interface"
                                        | "signed"
                                        | "unsigned"
                                        | "const"
                                        | "var"
                                        | "static"
                                        | "automatic"
                                ) =>
                        {
                            return Some(tok.text.to_string());
                        }
                        _ => {
                            if out.is_some() && tok.text.as_ref() != "::" {
                                return out;
                            }
                        }
                    }
                }
                Child::Node(k) => {
                    if out.is_some() {
                        return out;
                    }
                    if tree.kind(k) == K::NameExpr || tree.kind(k) == K::ScopedExpr {
                        let txt = tree.bare_text(k);
                        return Some(txt.rsplit("::").next().unwrap_or(&txt).to_string());
                    }
                }
            }
        }
        out
    }

    /// Comment right above the declaration's first token, or after its
    /// last on the same line.
    fn doc(&self, start: u32, end: u32) -> String {
        let before = &self.text[..start as usize];
        // Lines above, stopping at a blank or code line.
        let mut lines: Vec<&str> = Vec::new();
        let mut rest = before;
        // Drop the indentation on the declaration's own line.
        if let Some(nl) = rest.rfind('\n') {
            if rest[nl + 1..].trim().is_empty() {
                rest = &rest[..nl];
            } else {
                rest = "";
            }
        } else if !rest.trim().is_empty() {
            rest = "";
        }
        let mut in_block = false;
        while !rest.is_empty() || in_block {
            let (head, line) = match rest.rfind('\n') {
                Some(nl) => (&rest[..nl], &rest[nl + 1..]),
                None => ("", rest),
            };
            let t = line.trim();
            if in_block {
                lines.push(t);
                if t.starts_with("/*") {
                    in_block = false;
                }
            } else if t.starts_with("//") {
                lines.push(t);
            } else if t.ends_with("*/") {
                lines.push(t);
                in_block = !t.starts_with("/*");
            } else {
                break;
            }
            if head.is_empty() && rest.rfind('\n').is_none() {
                break;
            }
            rest = head;
        }
        lines.reverse();
        let mut doc: Vec<String> = lines
            .iter()
            .map(|l| clean_comment(l))
            .filter(|l| !l.chars().all(|c| matches!(c, '-' | '=' | '*' | '/' | ' ')))
            .collect();
        if doc.is_empty() {
            // `int count; // number of items`
            let after = &self.text[end as usize..];
            let line = after.split('\n').next().unwrap_or("");
            if let Some(i) = line.find("//") {
                if line[..i].trim().trim_start_matches(';').trim().is_empty()
                    || line[..i].trim().chars().all(|c| matches!(c, ';' | ',' | ')'))
                {
                    doc.push(clean_comment(&line[i..]));
                }
            }
        }
        doc.join("\n").trim().to_string()
    }

    fn push(&mut self, s: Symbol) -> u32 {
        self.syms.push(s);
        self.syms.len() as u32 - 1
    }

    fn decl(
        &mut self,
        n: NodeId,
        kind: SymKind,
        parent: Option<u32>,
        local: bool,
        name_node: Option<NodeId>,
    ) -> Option<u32> {
        let name_node = name_node.or_else(|| self.tree.child(n, K::Name))?;
        let (name, scope_class, ss, se) = self.name_of(name_node)?;
        let (start, end) = self.range(n).unwrap_or((ss, se));
        let stop = !matches!(kind, SymKind::Typedef);
        let mut detail = self.detail(n, stop);
        if kind.is_scope()
            || matches!(
                kind,
                SymKind::Function
                    | SymKind::Task
                    | SymKind::Covergroup
                    | SymKind::Clocking
                    | SymKind::Property
                    | SymKind::Sequence
                    | SymKind::Constraint
            )
        {
            detail = self.header(n);
        }
        let type_name = match kind {
            SymKind::Class => self
                .tree
                .child(n, K::Header)
                .and_then(|h| self.tree.child(h, K::Extends))
                .or_else(|| self.tree.child(n, K::Extends))
                .and_then(|e| self.tree.child(e, K::DataType))
                .and_then(|d| self.type_name(d)),
            SymKind::Function | SymKind::Task => self
                .tree
                .child(n, K::ReturnType)
                .and_then(|r| self.tree.child(r, K::DataType))
                .and_then(|d| self.type_name(d)),
            _ => None,
        };
        let doc = self.doc(start, end);
        Some(self.push(Symbol {
            name,
            kind,
            start,
            end,
            sel_start: ss,
            sel_end: se,
            parent,
            detail,
            type_name,
            scope_class,
            local,
            doc,
        }))
    }

    /// A scope's header: up to the end of its Header/PortList, else up to
    /// the first `;`.
    fn header(&self, n: NodeId) -> String {
        let tree = self.tree;
        let mut toks: Vec<u32> = Vec::new();
        for c in &tree.node(n).children {
            match *c {
                Child::Token(t) => toks.push(t),
                Child::Node(k) => {
                    if matches!(
                        tree.kind(k),
                        K::Body
                            | K::DataDecl
                            | K::FunctionDecl
                            | K::TaskDecl
                            | K::ClassDecl
                            | K::ConstraintBlock
                            | K::Coverpoint
                            | K::Cross
                    ) {
                        break;
                    }
                    toks.extend(tree.token_range(k));
                }
            }
        }
        join_tokens(
            toks.iter().map(|&t| &tree.tokens[t as usize]).filter(|t| !t.from_macro).map(|t| t.text.as_ref()),
            true,
        )
    }

    fn declarators(&mut self, n: NodeId, kind: SymKind, parent: Option<u32>, local: bool, ty: Option<String>) {
        let decls: Vec<NodeId> = self.tree.children(n).filter(|&c| self.tree.kind(c) == K::Declarator).collect();
        let detail = self.detail(n, true);
        for d in decls {
            if let Some(i) = self.decl(d, kind, parent, local, None) {
                let s = &mut self.syms[i as usize];
                s.type_name = ty.clone();
                s.detail = detail.clone();
                // The doc sits above the whole declaration.
                if let Some((a, b)) = self.range(n) {
                    let doc = self.doc(a, b);
                    let s = &mut self.syms[i as usize];
                    s.doc = doc;
                    s.start = a;
                    s.end = b;
                }
            }
        }
    }

    fn walk(&mut self, n: NodeId, parent: Option<u32>, local: bool) {
        let tree = self.tree;
        let kind = tree.kind(n);
        let scope = |k| match k {
            K::ModuleDecl => Some(SymKind::Module),
            K::InterfaceDecl => Some(SymKind::Interface),
            K::ProgramDecl => Some(SymKind::Program),
            K::PackageDecl => Some(SymKind::Package),
            K::CheckerDecl => Some(SymKind::Checker),
            K::ClassDecl => Some(SymKind::Class),
            K::CovergroupDecl => Some(SymKind::Covergroup),
            K::PropertyDecl => Some(SymKind::Property),
            K::SequenceDecl => Some(SymKind::Sequence),
            K::ClockingDecl => Some(SymKind::Clocking),
            K::ConstraintDecl | K::ConstraintPrototype => Some(SymKind::Constraint),
            K::TaskDecl => Some(SymKind::Task),
            K::FunctionDecl => Some(SymKind::Function),
            _ => None,
        };
        if let Some(sk) = scope(kind) {
            let me = self.decl(n, sk, parent, local, None);
            let inner_local = local || matches!(sk, SymKind::Function | SymKind::Task);
            for c in tree.children(n).collect::<Vec<_>>() {
                self.walk(c, me.or(parent), inner_local);
            }
            return;
        }
        match kind {
            K::MethodPrototype => {
                let is_task = tree.token_range(n).iter().any(|&t| tree.tokens[t as usize].text.as_ref() == "task");
                let me = self.decl(n, if is_task { SymKind::Task } else { SymKind::Function }, parent, local, None);
                if let Some(i) = me {
                    self.syms[i as usize].detail = self.detail(n, true);
                }
                // Prototype ports are not in any scope.
            }
            K::TypedefDecl => {
                // The name is the typedef's own Name child (the last one).
                let name = tree.children(n).filter(|&c| tree.kind(c) == K::Name).last();
                let me = self.decl(n, SymKind::Typedef, parent, local, name);
                if let Some(i) = me {
                    let dt = tree.child(n, K::DataType);
                    let ty = dt.and_then(|d| self.type_name(d));
                    self.syms[i as usize].type_name = ty;
                    if let Some(d) = dt {
                        self.enum_members(d, parent, local, Some(i));
                    }
                }
            }
            K::ForwardTypedef => {}
            K::DataDecl | K::ParamDecl | K::Port | K::GenvarDecl => {
                let sk = match kind {
                    K::ParamDecl => SymKind::Parameter,
                    K::Port => SymKind::Port,
                    K::GenvarDecl => SymKind::Genvar,
                    _ => SymKind::Variable,
                };
                let dt = tree.child(n, K::DataType);
                let ty = dt.and_then(|d| self.type_name(d));
                self.declarators(n, sk, parent, local, ty);
                if let Some(d) = dt {
                    self.enum_members(d, parent, local, None);
                }
                // Initializers may hold nothing we index; struct members are
                // DataDecls under the DataType.
                if let Some(d) = dt {
                    for c in tree.children(d).collect::<Vec<_>>() {
                        if tree.kind(c) == K::DataDecl {
                            self.walk(c, parent, true);
                        }
                    }
                }
            }
            K::Instantiation => {
                let ty = tree.child(n, K::DataType).and_then(|d| self.type_name(d));
                for inst in tree.children(n).filter(|&c| tree.kind(c) == K::Instance).collect::<Vec<_>>() {
                    if let Some(i) = self.decl(inst, SymKind::Instance, parent, local, None) {
                        self.syms[i as usize].type_name = ty.clone();
                        let d = self.detail(n, true);
                        self.syms[i as usize].detail = d;
                    }
                }
            }
            K::ModportDecl => {
                for c in tree.preorder_of(n) {
                    if tree.kind(c) == K::Name {
                        self.decl(c, SymKind::Modport, parent, local, Some(c));
                    }
                }
            }
            K::ImportDecl => {
                let txt = tree.bare_text(n);
                for part in txt.trim_start_matches("import").trim_end_matches(';').split(',') {
                    if let Some((pkg, _)) = part.split_once("::") {
                        self.imports.push(pkg.trim().to_string());
                    }
                }
            }
            K::Body | K::SeqBlock | K::ParBlock => {
                for c in tree.children(n).collect::<Vec<_>>() {
                    self.walk(c, parent, true);
                }
            }
            _ => {
                for c in tree.children(n).collect::<Vec<_>>() {
                    self.walk(c, parent, local);
                }
            }
        }
    }

    /// `enum {A, B}` members: Declarators directly under a DataType.
    fn enum_members(&mut self, dt: NodeId, parent: Option<u32>, local: bool, typedef: Option<u32>) {
        let tree = self.tree;
        let decls: Vec<NodeId> = tree.children(dt).filter(|&c| tree.kind(c) == K::Declarator).collect();
        for d in decls {
            if let Some(i) = self.decl(d, SymKind::EnumMember, typedef.or(parent), local, None) {
                if let Some(t) = typedef {
                    self.syms[i as usize].type_name = Some(self.syms[t as usize].name.clone());
                }
            }
        }
    }
}

trait Preorder {
    fn preorder_of(&self, n: NodeId) -> Vec<NodeId>;
}

impl Preorder for Tree {
    fn preorder_of(&self, n: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            out.push(x);
            let kids: Vec<NodeId> = self.children(x).collect();
            stack.extend(kids.into_iter().rev());
        }
        out
    }
}

/// Joins tokens with single spaces where code usually has them:
/// `function void f(int a, ref b q);`, `bit [7:0] x;`, `p::c`, `#(int)`.
fn join_tokens<'a>(toks: impl Iterator<Item = &'a str>, stop_at_semicolon: bool) -> String {
    let mut s = String::new();
    let mut depth = 0i32;
    let mut brackets = 0i32;
    let mut prev = "";
    for txt in toks {
        let word = |x: &str| {
            x.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '`' || c == '\'')
        };
        let glue = s.is_empty()
            || matches!(txt, ")" | "]" | "," | ";" | "::" | "." | "'")
            || matches!(prev, "(" | "[" | "::" | "." | "#" | "'" | "$")
            || (txt == "(" && (word(prev) || prev == "#"))
            || (txt == "["
                && word(prev)
                && !sv_syntax::is_builtin_type(prev)
                && !matches!(prev, "wire" | "reg" | "tri" | "signed" | "unsigned"))
            || (brackets > 0 && (txt == ":" || prev == ":"))
            || (word(txt) && prev.ends_with('\''));
        if !glue {
            s.push(' ');
        }
        s.push_str(txt);
        match txt {
            "(" | "{" => depth += 1,
            ")" | "}" => depth -= 1,
            "[" => brackets += 1,
            "]" => brackets -= 1,
            ";" if depth <= 0 && stop_at_semicolon => break,
            _ => {}
        }
        prev = txt;
        if s.len() > 300 {
            let mut cut = 300;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            s.truncate(cut);
            s.push_str(" ...");
            break;
        }
    }
    s
}

fn clean_comment(l: &str) -> String {
    let l = l.trim();
    let l = l.trim_start_matches("///").trim_start_matches("//!").trim_start_matches("//");
    let l = l.trim_start_matches("/**").trim_start_matches("/*");
    let l = l.trim_end_matches("*/");
    let l = l.trim();
    let l = l.strip_prefix("* ").unwrap_or(if l == "*" { "" } else { l });
    l.to_string()
}

/// `` `define NAME `` lines: macro symbols (the preprocessor consumes them).
fn macro_defs(text: &str, syms: &mut Vec<Symbol>, idents: &mut HashMap<String, Vec<u32>>) {
    let mut off = 0usize;
    let mut doc_lines: Vec<String> = Vec::new();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        let line_off = off;
        off += line.len();
        i += 1;
        if t.starts_with("//") {
            doc_lines.push(clean_comment(t));
            continue;
        }
        if let Some(rest) = t.strip_prefix("`define") {
            let ws = rest.len() - rest.trim_start().len();
            let rest2 = rest.trim_start();
            let n = rest2.bytes().take_while(|&b| crate::text::is_ident_byte(b)).count();
            if n > 0 && ws > 0 {
                let name = &rest2[..n];
                let name_off = (line_off + (line.len() - t.len()) + "`define".len() + ws) as u32;
                // Body continues over lines ending in a backslash.
                let mut end = line_off + line.trim_end().len();
                let mut detail = line.trim().to_string();
                let mut j = i;
                let mut cont = line.trim_end().ends_with('\\');
                let mut extra = 0;
                while cont && j < lines.len() {
                    let l = lines[j];
                    end = off + l.trim_end().len();
                    off += l.len();
                    if extra < 6 {
                        detail.push('\n');
                        detail.push_str(l.trim_end());
                    }
                    extra += 1;
                    cont = l.trim_end().ends_with('\\');
                    j += 1;
                }
                if extra >= 6 {
                    detail.push_str("\n ...");
                }
                i = j;
                idents.entry(format!("`{}", name)).or_default().push(name_off);
                syms.push(Symbol {
                    name: name.to_string(),
                    kind: SymKind::Macro,
                    start: (line_off + (line.len() - t.len())) as u32,
                    end: end as u32,
                    sel_start: name_off,
                    sel_end: name_off + n as u32,
                    parent: None,
                    detail,
                    type_name: None,
                    scope_class: None,
                    local: false,
                    doc: doc_lines.join("\n"),
                });
            }
        }
        doc_lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(src: &str) -> FileIndex {
        FileIndex::build(Path::new("t.sv"), src.to_string())
    }

    #[test]
    fn class_members_and_types() {
        let f = idx("package p;\n// An item.\nclass c extends base #(int);\n  int x, y; // two ints\n  local my_item item;\n  extern function void f(int a);\n  task run(); int k; endtask\nendclass\nfunction void c::f(int a); endfunction\nendpackage\n");
        let names: Vec<(&str, SymKind, bool)> = f.symbols.iter().map(|s| (s.name.as_str(), s.kind, s.local)).collect();
        assert!(names.contains(&("c", SymKind::Class, false)));
        assert!(names.contains(&("item", SymKind::Variable, false)));
        assert!(names.contains(&("k", SymKind::Variable, true)));
        let c = f.symbols.iter().find(|s| s.name == "c").unwrap();
        assert_eq!(c.type_name.as_deref(), Some("base"));
        assert_eq!(c.doc, "An item.");
        let item = f.symbols.iter().find(|s| s.name == "item").unwrap();
        assert_eq!(item.type_name.as_deref(), Some("my_item"));
        let x = f.symbols.iter().find(|s| s.name == "x").unwrap();
        assert_eq!(x.doc, "two ints");
        let fs: Vec<&Symbol> = f.symbols.iter().filter(|s| s.name == "f").collect();
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[1].scope_class.as_deref(), Some("c"));
        assert_eq!(&f.text[c.sel_start as usize..c.sel_end as usize], "c");
    }

    #[test]
    fn modules_enums_macros() {
        let f = idx("`define MAX(a,b) ((a)>(b)?(a):(b))\ntypedef enum {RED, GREEN} color_t;\nmodule m #(parameter W=8) (input logic clk);\n  sub u_sub(.a(clk));\n  int z = `MAX(1,2);\nendmodule\n");
        let get = |n: &str| f.symbols.iter().find(|s| s.name == n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(get("MAX").kind, SymKind::Macro);
        assert_eq!(get("RED").kind, SymKind::EnumMember);
        assert_eq!(get("color_t").kind, SymKind::Typedef);
        assert_eq!(get("W").kind, SymKind::Parameter);
        assert_eq!(get("clk").kind, SymKind::Port);
        assert_eq!(get("u_sub").type_name.as_deref(), Some("sub"));
        assert_eq!(f.idents["`MAX"].len(), 2);
    }
}
