//! Parser state: a cursor over the tokens and a list of events (start
//! node, token, finish node) that `build` turns into the tree. A finished
//! node can later be wrapped by a new parent (`Completed::precede`), which
//! the expression parser needs for binary operators.

use sv_lexer::Kind;

use crate::tree::{Child, Node, NodeId, SyntaxError, Token};
use crate::SyntaxKind;

#[derive(Debug)]
pub(crate) enum Event {
    Start { kind: SyntaxKind, forward_parent: u32 },
    Token,
    Finish,
    Abandoned,
}

pub(crate) struct Parser<'t> {
    toks: &'t [Token],
    pos: usize,
    pub(crate) events: Vec<Event>,
    pub(crate) errors: Vec<SyntaxError>,
}

pub(crate) struct Marker(usize);
#[derive(Clone, Copy)]
pub(crate) struct Completed(usize);

impl Marker {
    pub(crate) fn complete(self, p: &mut Parser, kind: SyntaxKind) -> Completed {
        if let Event::Start { kind: k, .. } = &mut p.events[self.0] {
            *k = kind;
        }
        p.events.push(Event::Finish);
        Completed(self.0)
    }

    /// Drops the marker; whatever it would have held goes to its parent.
    pub(crate) fn abandon(self, p: &mut Parser) {
        if self.0 == p.events.len() - 1 {
            p.events.pop();
        } else {
            p.events[self.0] = Event::Abandoned;
        }
    }
}

impl Completed {
    /// Starts a node that will become this one's parent.
    pub(crate) fn precede(self, p: &mut Parser) -> Marker {
        let m = p.start();
        if let Event::Start { forward_parent, .. } = &mut p.events[self.0] {
            *forward_parent = (m.0 - self.0) as u32;
        }
        m
    }
}

impl<'t> Parser<'t> {
    pub(crate) fn new(toks: &'t [Token]) -> Self {
        Parser { toks, pos: 0, events: Vec::with_capacity(toks.len() * 2), errors: Vec::new() }
    }

    pub(crate) fn start(&mut self) -> Marker {
        self.events.push(Event::Start { kind: SyntaxKind::Tombstone, forward_parent: 0 });
        Marker(self.events.len() - 1)
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Text of the token `n` ahead, "" past the end.
    pub(crate) fn nth(&self, n: usize) -> &'t str {
        self.toks.get(self.pos + n).map_or("", |t| &*t.text)
    }

    pub(crate) fn nth_kind(&self, n: usize) -> Option<Kind> {
        self.toks.get(self.pos + n).map(|t| t.kind)
    }

    /// Text of the token at absolute index `i`.
    pub(crate) fn text_at(&self, i: usize) -> &'t str {
        self.toks.get(i).map_or("", |t| &*t.text)
    }

    pub(crate) fn kind_at(&self, i: usize) -> Option<Kind> {
        self.toks.get(i).map(|t| t.kind)
    }

    pub(crate) fn cur(&self) -> &'t str {
        self.nth(0)
    }

    /// The current token is this keyword or punctuation. An identifier
    /// spelled like a keyword (under `begin_keywords) is not.
    pub(crate) fn at(&self, s: &str) -> bool {
        self.cur() == s && !self.at_ident()
    }

    pub(crate) fn at_any(&self, set: &[&str]) -> bool {
        set.contains(&self.cur()) && !self.at_ident()
    }

    pub(crate) fn at_ident(&self) -> bool {
        matches!(self.nth_kind(0), Some(Kind::Identifier | Kind::EscapedIdentifier))
    }

    /// Trivia came before token `n` ahead (macro bodies included).
    pub(crate) fn space_before(&self, n: usize) -> bool {
        self.toks.get(self.pos + n).is_none_or(|t| t.space_before)
    }

    /// No trivia between token `n` ahead and the one before it, as in `(*`.
    pub(crate) fn joined(&self, n: usize) -> bool {
        match (self.toks.get(self.pos + n - 1), self.toks.get(self.pos + n)) {
            (Some(a), Some(b)) => {
                a.origin.file == b.origin.file && a.origin.offset + a.text.len() as u32 == b.origin.offset
            }
            _ => false,
        }
    }

    pub(crate) fn bump(&mut self) {
        if !self.at_end() {
            self.pos += 1;
            self.events.push(Event::Token);
        }
    }

    pub(crate) fn eat(&mut self, s: &str) -> bool {
        if self.at(s) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, s: &str) -> bool {
        if self.eat(s) {
            true
        } else {
            self.error(format!("expected `{s}`"));
            false
        }
    }

    pub(crate) fn error(&mut self, message: String) {
        self.errors.push(SyntaxError { token: self.pos as u32, message });
    }
}

/// Turns the event list into nodes. Every token becomes a leaf once.
pub(crate) fn build(mut events: Vec<Event>) -> (Vec<Node>, NodeId) {
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut tok = 0u32;
    let mut root = None;
    let mut parents = Vec::new();
    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::Abandoned) {
            Event::Start { kind, forward_parent } => {
                if kind == SyntaxKind::Tombstone {
                    continue;
                }
                parents.clear();
                parents.push(kind);
                let mut j = i;
                let mut fp = forward_parent;
                while fp != 0 {
                    j += fp as usize;
                    match std::mem::replace(&mut events[j], Event::Abandoned) {
                        Event::Start { kind, forward_parent } => {
                            parents.push(kind);
                            fp = forward_parent;
                        }
                        _ => unreachable!("forward parent is not a start"),
                    }
                }
                for &k in parents.iter().rev() {
                    stack.push(Node { kind: k, children: Vec::new() });
                }
            }
            Event::Token => {
                stack.last_mut().expect("token outside any node").children.push(Child::Token(tok));
                tok += 1;
            }
            Event::Finish => {
                let n = stack.pop().expect("unbalanced finish");
                let id = NodeId(nodes.len() as u32);
                nodes.push(n);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Child::Node(id)),
                    None => root = Some(id),
                }
            }
            Event::Abandoned => {}
        }
    }
    (nodes, root.expect("no root node"))
}
