//! Integer value sets for the analyzers: sorted, disjoint closed intervals,
//! and the ranges declared integral types allow.

use crate::syntax::Cst;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// A set of integers as sorted, disjoint, non-adjacent closed intervals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Set(pub Vec<(i128, i128)>);

impl Set {
    pub fn empty() -> Set {
        Set(Vec::new())
    }
    pub fn range(lo: i128, hi: i128) -> Set {
        if lo > hi {
            Set::empty()
        } else {
            Set(vec![(lo, hi)])
        }
    }
    pub fn point(v: i128) -> Set {
        Set::range(v, v)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn union(&self, o: &Set) -> Set {
        let mut v: Vec<(i128, i128)> = self.0.iter().chain(o.0.iter()).copied().collect();
        v.sort();
        let mut out: Vec<(i128, i128)> = Vec::new();
        for (lo, hi) in v {
            match out.last_mut() {
                Some(last) if lo <= last.1.saturating_add(1) => last.1 = last.1.max(hi),
                _ => out.push((lo, hi)),
            }
        }
        Set(out)
    }
    pub fn intersect(&self, o: &Set) -> Set {
        let mut out = Vec::new();
        for &(a, b) in &self.0 {
            for &(c, d) in &o.0 {
                let (lo, hi) = (a.max(c), b.min(d));
                if lo <= hi {
                    out.push((lo, hi));
                }
            }
        }
        Set(out).union(&Set::empty())
    }
    pub fn complement(&self) -> Set {
        let mut out = Vec::new();
        let mut next = i128::MIN;
        let mut open = true;
        for &(lo, hi) in &self.0 {
            if lo > next {
                out.push((next, lo - 1));
            }
            if hi == i128::MAX {
                open = false;
                break;
            }
            next = hi + 1;
        }
        if open {
            out.push((next, i128::MAX));
        }
        Set(out)
    }
    /// How many values (saturating).
    pub fn count(&self) -> u128 {
        self.0.iter().fold(0u128, |n, &(lo, hi)| n.saturating_add((hi - lo) as u128).saturating_add(1))
    }
    /// `[1:3], 7` with the open ends of an unbounded set as `$`.
    pub fn show(&self) -> String {
        if self.is_empty() {
            return "no value".into();
        }
        let b = |v: i128| match v {
            i128::MIN | i128::MAX => "$".to_string(),
            v => v.to_string(),
        };
        let mut parts: Vec<String> = self
            .0
            .iter()
            .take(6)
            .map(|&(lo, hi)| if lo == hi { b(lo) } else { format!("[{}:{}]", b(lo), b(hi)) })
            .collect();
        if self.0.len() > 6 {
            parts.push("...".into());
        }
        format!("{{{}}}", parts.join(", "))
    }
}

/// The value of a constant integer expression: a literal, `-` a literal,
/// or one in parentheses.
pub(crate) fn const_int(c: &Cst, n: NodeId) -> Option<i128> {
    match c.kind(n) {
        Literal => {
            let text: String = c.tree.token_range(n).iter().map(|&t| c.text_of(t)).collect::<String>().replace('_', "");
            let v = match text.find('\'') {
                None => text.parse::<u128>().ok()?,
                Some(i) => {
                    let d = text[i + 1..].trim_start_matches(['s', 'S']);
                    let (radix, rest) = match d.chars().next()? {
                        'd' | 'D' => (10, &d[1..]),
                        'h' | 'H' => (16, &d[1..]),
                        'o' | 'O' => (8, &d[1..]),
                        'b' | 'B' => (2, &d[1..]),
                        _ => return None,
                    };
                    u128::from_str_radix(rest, radix).ok()?
                }
            };
            i128::try_from(v).ok()
        }
        PrefixExpr if c.toks(n).next().map(|t| c.text_of(t)) == Some("-") => {
            let inner = c.tree.children(n).next()?;
            const_int(c, inner).map(|v| -v)
        }
        ParenExpr => const_int(c, c.tree.children(n).next()?),
        _ => None,
    }
}

/// The values a declared integral type holds, from its `DataType` node:
/// `bit [3:0]` is [0:15], `int` [-2^31:2^31-1]. None for any other type.
pub(crate) fn type_range(c: &Cst, dt: NodeId) -> Option<(Set, u32)> {
    let toks: Vec<&str> = c.toks(dt).map(|t| c.text_of(t)).collect();
    let (mut w, mut signed) = match *toks.first()? {
        "bit" | "logic" | "reg" => (1u32, false),
        "byte" => (8, true),
        "shortint" => (16, true),
        "int" | "integer" => (32, true),
        "longint" => (64, true),
        _ => return None,
    };
    if toks.contains(&"signed") {
        signed = true;
    }
    if toks.contains(&"unsigned") {
        signed = false;
    }
    let dims: Vec<NodeId> = c.tree.children(dt).filter(|&d| c.kind(d) == Dim).collect();
    if !dims.is_empty() {
        if w != 1 {
            return None;
        }
        w = 1;
        for d in dims {
            let k: Vec<NodeId> = c.tree.children(d).collect();
            let [a, b] = k[..] else { return None };
            let (a, b) = (const_int(c, a)?, const_int(c, b)?);
            w = w.checked_mul(u32::try_from((a - b).abs() + 1).ok()?)?;
        }
    }
    if w == 0 || w > 64 {
        return None;
    }
    let set =
        if signed { Set::range(-(1i128 << (w - 1)), (1i128 << (w - 1)) - 1) } else { Set::range(0, (1i128 << w) - 1) };
    Some((set, w))
}
