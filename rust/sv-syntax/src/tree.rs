//! The tree: a flat token array and a node arena. Every token produced by
//! the preprocessor is a leaf exactly once, in order. A token read from a
//! file carries the source text between it and the previous token of that
//! file (whitespace, comments, directives, macro uses) as leading trivia,
//! so each file is given back byte for byte; tokens a macro produced have
//! no trivia of their own.

use std::path::PathBuf;
use std::rc::Rc;

use sv_lexer::Kind;
use sv_preproc::Origin;

use crate::SyntaxKind;

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: Rc<str>,
    pub origin: Origin,
    /// Made by a macro expansion (or `__FILE__`/`__LINE__`).
    pub from_macro: bool,
    /// Start of the leading trivia in the token's file; the trivia ends at
    /// `origin.offset`. Equal to it when there is none.
    pub trivia_start: u32,
    /// Where the text was written (in a `define body or macro argument for
    /// expanded tokens); see `sv_preproc::PpToken::written`.
    pub written: sv_preproc::Origin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Node(NodeId),
    Token(u32),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: SyntaxKind,
    pub children: Vec<Child>,
}

#[derive(Clone, Debug)]
pub struct SyntaxError {
    /// Index of the token where the error was found (may equal the token
    /// count for the end of input).
    pub token: u32,
    pub message: String,
}

pub struct Tree {
    pub tokens: Vec<Token>,
    pub nodes: Vec<Node>,
    pub root: NodeId,
    pub files: Vec<PathBuf>,
    pub sources: Vec<Rc<str>>,
    pub errors: Vec<SyntaxError>,
    pub pp_diagnostics: Vec<sv_preproc::Diagnostic>,
}

impl Tree {
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn kind(&self, id: NodeId) -> SyntaxKind {
        self.node(id).kind
    }

    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.node(id).children.iter().filter_map(|c| match c {
            Child::Node(n) => Some(*n),
            Child::Token(_) => None,
        })
    }

    pub fn child(&self, id: NodeId, kind: SyntaxKind) -> Option<NodeId> {
        self.children(id).find(|&c| self.kind(c) == kind)
    }

    /// Token indices under a node, in order.
    pub fn token_range(&self, id: NodeId) -> Vec<u32> {
        let mut out = Vec::new();
        self.collect(id, &mut out);
        out
    }

    fn collect(&self, id: NodeId, out: &mut Vec<u32>) {
        for c in &self.node(id).children {
            match *c {
                Child::Token(t) => out.push(t),
                Child::Node(n) => self.collect(n, out),
            }
        }
    }

    /// The node's tokens joined without trivia: `uvm_object::print`.
    pub fn bare_text(&self, id: NodeId) -> String {
        self.token_range(id).iter().map(|&t| &*self.tokens[t as usize].text).collect()
    }

    pub fn trivia(&self, t: u32) -> &str {
        let tok = &self.tokens[t as usize];
        &self.sources[tok.origin.file as usize][tok.trivia_start as usize..tok.origin.offset as usize]
    }

    /// The node's text as written, trivia included, for tokens of one file.
    pub fn text(&self, id: NodeId) -> String {
        let mut s = String::new();
        for t in self.token_range(id) {
            let tok = &self.tokens[t as usize];
            if !tok.from_macro {
                s.push_str(self.trivia(t));
                s.push_str(&tok.text);
            }
        }
        s
    }

    /// Rebuilds file `file` from the tree's leaves alone.
    pub fn file_text(&self, file: u32) -> String {
        let src = &self.sources[file as usize];
        let mut s = String::with_capacity(src.len());
        let mut end = 0usize;
        self.walk_tokens(self.root, &mut |t| {
            let tok = &self.tokens[t as usize];
            if tok.origin.file == file && !tok.from_macro {
                s.push_str(self.trivia(t));
                s.push_str(&tok.text);
                end = tok.origin.offset as usize + tok.text.len();
            }
        });
        s.push_str(&src[end..]);
        s
    }

    fn walk_tokens(&self, id: NodeId, f: &mut impl FnMut(u32)) {
        for c in &self.node(id).children {
            match *c {
                Child::Token(t) => f(t),
                Child::Node(n) => self.walk_tokens(n, f),
            }
        }
    }

    /// Pre-order walk of the nodes.
    pub fn preorder(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(n) = stack.pop() {
            out.push(n);
            let kids: Vec<NodeId> = self.children(n).collect();
            stack.extend(kids.into_iter().rev());
        }
        out
    }

    /// An indented dump of the tree, for tests and debugging.
    pub fn debug_dump(&self) -> String {
        let mut s = String::new();
        self.dump(self.root, 0, &mut s);
        s
    }

    fn dump(&self, id: NodeId, depth: usize, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(s, "{}{:?}", "  ".repeat(depth), self.kind(id));
        for c in &self.node(id).children {
            match *c {
                Child::Node(n) => self.dump(n, depth + 1, s),
                Child::Token(t) => {
                    let _ = writeln!(s, "{}{:?}", "  ".repeat(depth + 1), &*self.tokens[t as usize].text);
                }
            }
        }
    }
}

/// Converts preprocessor tokens, filling in leading trivia per file.
pub(crate) fn tokens_from(out: &sv_preproc::Output) -> Vec<Token> {
    let mut last_end = vec![0u32; out.sources.len()];
    out.tokens
        .iter()
        .map(|t| {
            let mut trivia_start = t.origin.offset;
            if !t.from_macro {
                let f = t.origin.file as usize;
                trivia_start = last_end[f].min(t.origin.offset);
                last_end[f] = t.origin.offset + t.text.len() as u32;
            }
            Token { kind: t.kind, text: t.text.clone(), origin: t.origin, from_macro: t.from_macro, trivia_start, written: t.written }
        })
        .collect()
}
