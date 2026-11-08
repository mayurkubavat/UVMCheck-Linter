//! Rules that read only the syntax tree, ported from the C++ rules that walk
//! slang's syntax with a `SyntaxVisitor`. They run over `sv-syntax` trees.
//!
//! sv-syntax has fewer node kinds than slang (a case statement's items are
//! flat children, an assignment is a `BinaryExpr` with an `=` token), so each
//! port names the slang shape it stands for. Messages are verbatim.

use std::cell::RefCell;

use dv_model::Loc;

use sv_sema::Locator;

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::Sink;

use SyntaxKind::*;

/// One tree, with a pre-order of its nodes and their parents, and the
/// locator that numbers files as the design does.
pub struct Cst<'t> {
    /// The tree.
    pub tree: &'t Tree,
    pub(crate) ix: usize,
    pub(crate) pre: Vec<NodeId>,
    pub(crate) parent: Vec<u32>,
    pub(crate) locator: RefCell<Locator>,
    /// Files earlier trees of the compile read: slang does not read an
    /// include-guarded header again.
    pub prior_files: std::rc::Rc<std::collections::HashSet<std::path::PathBuf>>,
    /// Every tree of the compile (this one at `ix`), for checks that see
    /// names other files declare.
    pub all: &'t [Tree],
    /// The nodes of each kind in preorder, made on first use.
    pub(crate) by_kind: std::cell::OnceCell<Vec<Vec<NodeId>>>,
}

pub(crate) const NO_PARENT: u32 = u32::MAX;

impl<'t> Cst<'t> {
    /// Wraps tree `tree`, index `ix` of its compile, numbering files with
    /// `locator`. [`Self::all`] is just this tree until the caller sets it.
    pub fn new(tree: &'t Tree, ix: usize, locator: Locator) -> Self {
        let pre = tree.preorder();
        let mut parent = vec![NO_PARENT; tree.nodes.len()];
        for &n in &pre {
            for c in tree.children(n) {
                parent[c.0 as usize] = n.0;
            }
        }
        Cst {
            tree,
            ix,
            pre,
            parent,
            locator: RefCell::new(locator),
            prior_files: Default::default(),
            all: std::slice::from_ref(tree),
            by_kind: Default::default(),
        }
    }

    /// How many specializations slang elaborates of the generic class
    /// declared at `class` in this tree (None for a class that is not
    /// generic).
    pub fn spec_count(&self, class: NodeId) -> Option<usize> {
        crate::shared::get("spec_counts", self.compile_key(), || sv_sema::spec_counts(self.all))
            .get(&(self.index(), class))
            .copied()
    }

    /// Identifies this tree within its compile, for per-tree caches whose
    /// values depend on the other trees too.
    pub fn tree_key(&self) -> (usize, usize) {
        let (a, b) = self.compile_key();
        let t = (self.tree.nodes.len() << 20) ^ self.tree.tokens.len();
        (self.tree as *const Tree as usize, a.rotate_left(17) ^ b.wrapping_mul(0x9E37_79B9) ^ t)
    }

    /// Identifies the compile for per-compile caches: the trees' address
    /// and their sizes (an address alone can be reused by a later compile
    /// in the same thread).
    pub fn compile_key(&self) -> (usize, usize) {
        let size = self.all.iter().fold(self.all.len(), |a, t| {
            a.wrapping_mul(31).wrapping_add(t.nodes.len()).wrapping_mul(131).wrapping_add(t.tokens.len())
        });
        (self.all.as_ptr() as usize, size)
    }

    /// This tree's index in [`Self::all`].
    pub fn index(&self) -> usize {
        if self.all.len() == 1 {
            0
        } else {
            self.ix
        }
    }

    /// Gives back the locator, with any files this tree added.
    pub fn into_locator(self) -> Locator {
        self.locator.into_inner()
    }

    /// Nodes of a kind, in source order.
    pub fn nodes(&self, kind: SyntaxKind) -> Vec<NodeId> {
        self.kind_nodes(kind).to_vec()
    }

    /// The nodes of `kind` by the text of their `Name` child, each list in
    /// tree order (built once per tree and kind).
    pub fn named(&self, kind: SyntaxKind) -> std::sync::Arc<std::collections::HashMap<String, Vec<NodeId>>> {
        let (a, b) = self.tree_key();
        crate::shared::get("named", (a, b ^ ((kind as u16 as usize) << 48)), || {
            let mut out: std::collections::HashMap<String, Vec<NodeId>> = std::collections::HashMap::new();
            for &n in self.kind_nodes(kind) {
                if let Some(x) = self.tree.child(n, Name) {
                    out.entry(self.tree.bare_text(x)).or_default().push(n);
                }
            }
            out
        })
    }

    /// The first node of `kind` whose `Name` child reads `name`.
    pub fn first_named(&self, kind: SyntaxKind, name: &str) -> Option<NodeId> {
        self.named(kind).get(name).and_then(|v| v.first().copied())
    }

    /// [`Self::nodes`] without the copy.
    pub fn kind_nodes(&self, kind: SyntaxKind) -> &[NodeId] {
        let by = self.by_kind.get_or_init(|| {
            let mut v: Vec<Vec<NodeId>> = Vec::new();
            for &n in &self.pre {
                let k = self.tree.kind(n) as u16 as usize;
                if v.len() <= k {
                    v.resize_with(k + 1, Vec::new);
                }
                v[k].push(n);
            }
            v
        });
        by.get(kind as u16 as usize).map_or(&[], Vec::as_slice)
    }

    /// The kind of node `n`.
    pub fn kind(&self, n: NodeId) -> SyntaxKind {
        self.tree.kind(n)
    }

    /// The parent of node `n` (`None` for the root).
    pub fn parent(&self, n: NodeId) -> Option<NodeId> {
        let p = self.parent[n.0 as usize];
        (p != NO_PARENT).then_some(NodeId(p))
    }

    /// The children of node `n`, tokens and nodes, in order.
    pub fn kids(&self, n: NodeId) -> &'t [Child] {
        &self.tree.node(n).children
    }

    /// Child nodes, skipping labels and attributes (slang keeps those
    /// outside a block's items and a statement's kind).
    pub fn items(&self, n: NodeId) -> Vec<NodeId> {
        self.tree.children(n).filter(|&c| !matches!(self.kind(c), EndLabel | StmtLabel | Attribute)).collect()
    }

    /// Tokens that are direct children of `n`.
    pub fn toks(&self, n: NodeId) -> impl Iterator<Item = u32> + 't {
        self.tree.node(n).children.iter().filter_map(|c| match *c {
            Child::Token(t) => Some(t),
            Child::Node(_) => None,
        })
    }

    /// The text of token `tok`.
    pub fn text_of(&self, tok: u32) -> &'t str {
        &self.tree.tokens[tok as usize].text
    }

    /// First direct token with this text.
    pub fn tok(&self, n: NodeId, text: &str) -> Option<u32> {
        self.toks(n).find(|&t| self.text_of(t) == text)
    }

    /// The first direct token (a statement's or block's keyword).
    pub fn keyword(&self, n: NodeId) -> Option<u32> {
        self.toks(n).next()
    }

    /// The first token of node `n` (`None` for a node with no tokens).
    pub fn first_tok(&self, n: NodeId) -> Option<u32> {
        self.tree.token_bounds(n).map(|(a, _)| a)
    }

    /// The text of node `n`'s last token (empty when it has none).
    pub fn last_text(&self, n: NodeId) -> &'t str {
        self.tree.token_bounds(n).map(|(_, b)| self.text_of(b)).unwrap_or("")
    }

    /// slang's `toString()`: the text with each token's leading trivia
    /// (none for macro-made tokens here).
    pub fn text(&self, n: NodeId) -> String {
        let mut s = String::new();
        for t in self.tree.token_range(n) {
            let tok = &self.tree.tokens[t as usize];
            if !tok.from_macro {
                s.push_str(self.tree.trivia(t));
            } else if tok.space_before {
                // slang keeps a macro body's trivia on the expanded tokens,
                // the first one's too (`$cast(X , f())`: " f()").
                s.push(' ');
            }
            s.push_str(&tok.text);
        }
        s
    }

    /// A fix replacing bytes `start..end` of the tree's file `file`.
    pub fn file_fix(
        &self,
        file: u32,
        start: usize,
        end: usize,
        text: impl Into<String>,
        description: &str,
    ) -> crate::fix::Fix {
        crate::fix::Fix {
            path: self.tree.files.get(file as usize).map(|p| p.display().to_string()).unwrap_or_default(),
            start: start as u32,
            end: end as u32,
            text: text.into(),
            description: description.to_string(),
            safety: crate::fix::FixSafety::Safe,
        }
    }

    /// A fix replacing the written text of tokens `first..=last`; None when
    /// either comes from a macro expansion or they are in different files
    /// (no single place in a file to rewrite).
    pub fn token_fix(
        &self,
        first: u32,
        last: u32,
        text: impl Into<String>,
        description: &str,
    ) -> Option<crate::fix::Fix> {
        let (a, b) = (self.tree.tokens.get(first as usize)?, self.tree.tokens.get(last as usize)?);
        if a.from_macro || b.from_macro || a.origin.file != b.origin.file {
            return None;
        }
        let end = b.origin.offset as usize + b.text.len();
        Some(self.file_fix(a.origin.file, a.origin.offset as usize, end, text, description))
    }

    /// [`Self::text`] with surrounding whitespace trimmed.
    pub fn trimmed(&self, n: NodeId) -> String {
        self.text(n).trim_matches([' ', '\t', '\n', '\r']).to_string()
    }

    /// Location of a preprocessor position (not in a macro).
    pub fn loc_at(&self, o: sv_syntax::Origin) -> Loc {
        let (file, line, column) = self.locator.borrow_mut().position(self.ix, self.tree, o);
        Loc { file, line, column, expansion: None }
    }

    /// Location of token `tok`, with the expansion site for macro-made tokens
    /// (see `sv_sema::Locator::loc`).
    pub fn loc(&self, tok: u32) -> Loc {
        self.locator.borrow_mut().loc(self.ix, self.tree, tok)
    }

    /// Where a preprocessor diagnostic points.
    pub fn origin_loc(&self, o: sv_syntax::Origin) -> Loc {
        let (file, line, column) = self.locator.borrow_mut().position(self.ix, self.tree, o);
        Loc { file, line, column, expansion: None }
    }

    /// The node after token `tok` among `n`'s children.
    pub fn node_after(&self, n: NodeId, tok: u32) -> Option<NodeId> {
        let kids = self.kids(n);
        let i = kids.iter().position(|c| *c == Child::Token(tok))?;
        kids[i + 1..].iter().find_map(|c| match *c {
            Child::Node(x) => Some(x),
            Child::Token(_) => None,
        })
    }
}

/// A rule that reads one syntax tree.
pub type SyntaxFn = fn(&Cst, &mut Sink);

/// slang's `CaseStatementSyntax`: case, casex, casez (randcase is another kind).
pub(crate) fn case_statements(c: &Cst) -> Vec<(NodeId, u32)> {
    c.nodes(CaseStmt)
        .into_iter()
        .filter_map(|n| {
            let kw = c.toks(n).find(|&t| matches!(c.text_of(t), "case" | "casex" | "casez"))?;
            Some((n, kw))
        })
        .collect()
}

pub(crate) const PROCEDURAL_KEYWORDS: [&str; 6] =
    ["initial", "final", "always", "always_comb", "always_ff", "always_latch"];

/// slang's `ProceduralBlockSyntax` and its keyword.
pub(crate) fn procedural_blocks(c: &Cst) -> Vec<(NodeId, u32)> {
    c.nodes(ProceduralBlock)
        .into_iter()
        .filter_map(|n| {
            let kw = c.toks(n).find(|&t| PROCEDURAL_KEYWORDS.contains(&c.text_of(t)))?;
            Some((n, kw))
        })
        .collect()
}

/// slang's `AssignmentExpression` (plain `=`): its operator, left and right.
pub(crate) fn assignments(c: &Cst) -> Vec<(u32, NodeId, NodeId)> {
    c.nodes(BinaryExpr)
        .into_iter()
        .filter_map(|n| {
            let op = c.toks(n).next()?;
            if c.text_of(op) != "=" {
                return None;
            }
            let mut sides = c.tree.children(n);
            Some((op, sides.next()?, sides.next()?))
        })
        .collect()
}
