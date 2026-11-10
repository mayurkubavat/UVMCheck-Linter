//! User-defined primitives (IEEE 1800 chapter 29): what a primitive's
//! header, port declarations, `initial` statement and table may hold.
//!
//! - one output, first, and at least one input (29.3);
//! - an ANSI header has no port declarations in the body; a non-ANSI one
//!   declares each port once in the body, and only its own ports (29.3.2);
//! - `reg` only on the output; `initial` only in a sequential primitive,
//!   on the output, with a 1-bit value (29.4, 29.5);
//! - table rows: one symbol per input, a state field exactly when the
//!   primitive is sequential, valid level symbols, `-` only as a
//!   sequential output, at most one edge, only on an input, only in a
//!   sequential primitive, between two different values (29.3.6);
//! - two rows that match the same inputs with different outputs (29.3.6);
//! - gate instances with the wrong number of terminals or an empty one
//!   (28.3), and primitive instances with more than two delays (29.7).

use crate::fx::FxSet as HashSet;

use sv_syntax::{NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

/// Runs this module's checks over every tree of a compile and returns the
/// errors found (user-defined primitive errors), in tree and source order.
pub fn check(trees: &[Tree]) -> Vec<Diag> {
    let mut out = Vec::new();
    let mut udps = HashSet::default();
    for t in trees {
        for n in t.preorder() {
            if t.kind(n) == UdpDecl {
                if let Some(nm) = t.child(n, Name) {
                    udps.insert(t.bare_text(nm));
                }
            }
        }
    }
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            match t.kind(n) {
                UdpDecl => Udp { t, ix, out: &mut out }.check(n),
                GateInst => gate(t, ix, n, &mut out),
                Instantiation => {
                    let Some(dt) = t.child(n, DataType) else { continue };
                    if !udps.contains(&text(t, dt).first().cloned().unwrap_or_default()) {
                        continue;
                    }
                    if let Some(pl) = t.child(dt, ParamList) {
                        if args(t, pl).len() > 2 {
                            out.push(Diag {
                                tree: ix,
                                node: pl,
                                message: "a primitive instance can have at most two delay values".into(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The comma-separated entries of a parenthesized list (or of the one
/// `ArgList` in it), each as its tokens.
fn args(t: &Tree, n: NodeId) -> Vec<Vec<String>> {
    let toks = text(t, n);
    let open = toks.iter().position(|x| x == "(").unwrap_or(0);
    let mut out = vec![Vec::new()];
    let mut depth = 0;
    for x in &toks[open + 1..] {
        match x.as_str() {
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" if depth == 0 => break,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                out.push(Vec::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(x.clone());
    }
    out
}

/// Terminal counts of the built-in gates: (fewest, most).
fn terminals(gate: &str) -> Option<(usize, usize)> {
    Some(match gate {
        "and" | "nand" | "or" | "nor" | "xor" | "xnor" | "buf" | "not" => (2, usize::MAX),
        "bufif0" | "bufif1" | "notif0" | "notif1" | "nmos" | "pmos" | "rnmos" | "rpmos" => (3, 3),
        "tranif0" | "tranif1" | "rtranif0" | "rtranif1" => (3, 3),
        "cmos" | "rcmos" => (4, 4),
        "tran" | "rtran" => (2, 2),
        "pullup" | "pulldown" => (1, 1),
        _ => return None,
    })
}

fn gate(t: &Tree, ix: usize, n: NodeId, out: &mut Vec<Diag>) {
    let Some(kind) = text(t, n).first().cloned() else { return };
    let Some((lo, hi)) = terminals(&kind) else { return };
    for inst in t.children(n).filter(|&c| t.kind(c) == Instance) {
        let Some(al) = t.child(inst, ArgList) else { continue };
        let a = args(t, al);
        let a: Vec<Vec<String>> = if a.len() == 1 && a[0].is_empty() { Vec::new() } else { a };
        if a.iter().any(|x| x.is_empty()) {
            out.push(Diag {
                tree: ix,
                node: inst,
                message: "only simple expressions are allowed for primitive port connections".into(),
            });
        } else if a.len() < lo || a.len() > hi {
            let message = if hi == usize::MAX {
                format!("instance of '{kind}' requires at least one input and one output connection")
            } else {
                format!("wrong number of port connections for '{kind}' instance ({} given, expected {lo})", a.len())
            };
            out.push(Diag { tree: ix, node: inst, message });
        }
    }
}

fn text(t: &Tree, n: NodeId) -> Vec<String> {
    t.token_range(n).iter().map(|&i| t.tokens[i as usize].text.to_string()).collect()
}

struct Udp<'a> {
    t: &'a Tree,
    ix: usize,
    out: &'a mut Vec<Diag>,
}

#[derive(Default)]
struct Port {
    name: String,
    output: bool,
    input: bool,
    reg: bool,
    init: bool,
}

/// One table field: a set of levels or of edges (from, to).
#[derive(Clone, PartialEq)]
enum Field {
    Levels(Vec<u8>),
    Edges(Vec<(u8, u8)>),
}

const LEVELS: [u8; 3] = *b"01x";

fn levels(c: u8) -> Option<Vec<u8>> {
    Some(match c.to_ascii_lowercase() {
        b'0' => vec![b'0'],
        b'1' => vec![b'1'],
        b'x' => vec![b'x'],
        b'?' => LEVELS.to_vec(),
        b'b' => vec![b'0', b'1'],
        _ => return None,
    })
}

fn edges_of(from: &[u8], to: &[u8]) -> Vec<(u8, u8)> {
    let mut v = Vec::new();
    for &a in from {
        for &b in to {
            if a != b {
                v.push((a, b));
            }
        }
    }
    v
}

fn edge_symbol(c: u8) -> Option<Vec<(u8, u8)>> {
    Some(match c.to_ascii_lowercase() {
        b'r' => vec![(b'0', b'1')],
        b'f' => vec![(b'1', b'0')],
        b'p' => vec![(b'0', b'1'), (b'0', b'x'), (b'x', b'1')],
        b'n' => vec![(b'1', b'0'), (b'1', b'x'), (b'x', b'0')],
        b'*' => edges_of(&LEVELS, &LEVELS),
        _ => return None,
    })
}

fn overlaps(a: &Field, b: &Field) -> bool {
    match (a, b) {
        (Field::Levels(x), Field::Levels(y)) => x.iter().any(|c| y.contains(c)),
        (Field::Edges(x), Field::Edges(y)) => x.iter().any(|c| y.contains(c)),
        _ => false,
    }
}

impl Udp<'_> {
    fn err(&mut self, node: NodeId, message: String) {
        self.out.push(Diag { tree: self.ix, node, message });
    }

    fn check(&mut self, n: NodeId) {
        let t = self.t;
        let header: Vec<String> = t.child(n, ParenList).map(|p| text(t, p)).unwrap_or_default();
        // `primitive p(.*);`: the ports of its `extern` declaration.
        let wildcard = header.iter().any(|x| x == "*");
        let ansi = header.iter().any(|x| matches!(x.as_str(), "input" | "output"));
        let mut ports: Vec<Port> = Vec::new();
        if ansi {
            let (mut dir, mut reg, mut value) = ("", false, false);
            for x in &header {
                if value && x != "," {
                    continue;
                }
                value = false;
                match x.as_str() {
                    "(" | ")" | "," => {}
                    "input" | "output" => {
                        dir = if x == "input" { "input" } else { "output" };
                        reg = false;
                    }
                    "reg" => reg = true,
                    "=" => {
                        value = true;
                        if let Some(p) = ports.last_mut() {
                            p.init = true;
                        }
                    }
                    s if s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '\\') => {
                        ports.push(Port {
                            name: s.to_string(),
                            output: dir == "output",
                            input: dir == "input",
                            reg,
                            init: false,
                        });
                    }
                    _ => {}
                }
            }
        } else {
            for x in header.iter().filter(|_| !wildcard) {
                if !matches!(x.as_str(), "(" | ")" | ",") {
                    ports.push(Port { name: x.clone(), ..Default::default() });
                }
            }
        }
        let mut declared: HashSet<(String, &str)> = HashSet::default();
        let mut initial_target: Option<(NodeId, String, Vec<String>)> = None;
        let mut table = None;
        for c in t.children(n) {
            match t.kind(c) {
                UdpPortDecl => {
                    if ansi {
                        self.err(
                            c,
                            "primitives cannot mix an ansi-style header with port declarations inside their bodies"
                                .into(),
                        );
                        continue;
                    }
                    let toks = text(t, c);
                    let mut kind = "";
                    let mut reg = false;
                    let mut after_eq = false;
                    for x in &toks {
                        match x.as_str() {
                            "input" | "output" => kind = if x == "input" { "input" } else { "output" },
                            "reg" => reg = true,
                            "," => after_eq = false,
                            "=" => after_eq = true,
                            ";" => {}
                            name if !after_eq && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') => {
                                if wildcard && !ports.iter().any(|p| p.name == name) {
                                    ports.push(Port { name: name.to_string(), ..Default::default() });
                                }
                                let Some(p) = ports.iter_mut().find(|p| p.name == name) else {
                                    self.err(c, format!("port declaration '{name}' does not match any port in the primitive's port list"));
                                    continue;
                                };
                                if !kind.is_empty() {
                                    let clash = (kind == "input" && p.output) || (kind == "output" && p.input);
                                    if !declared.insert((name.to_string(), kind)) || clash {
                                        self.err(c, format!("port '{name}' already declared"));
                                    }
                                    if kind == "input" {
                                        p.input = true;
                                    } else {
                                        p.output = true;
                                    }
                                }
                                if reg {
                                    if !declared.insert((name.to_string(), "reg")) {
                                        self.err(c, format!("port '{name}' already declared"));
                                    }
                                    p.reg = true;
                                }
                            }
                            _ => {}
                        }
                    }
                    if toks.contains(&"=".to_string()) {
                        for p in ports.iter_mut().filter(|p| p.output) {
                            p.init = true;
                        }
                    }
                }
                UdpInitial => {
                    let toks = text(t, c);
                    let name = toks.get(1).cloned().unwrap_or_default();
                    let value: Vec<String> =
                        toks.iter().skip_while(|x| *x != "=").skip(1).take_while(|x| *x != ";").cloned().collect();
                    initial_target = Some((c, name, value));
                }
                UdpTable => table = Some(c),
                _ => {}
            }
        }
        if !ansi {
            for p in &ports {
                if !p.input && !p.output {
                    self.err(n, format!("port '{}' is missing a corresponding body declaration", p.name));
                }
            }
        }
        for p in &ports {
            if p.reg && !p.output && (p.input || ansi) {
                self.err(n, format!("'reg' can only be applied to the output of a primitive, not '{}'", p.name));
            }
        }
        let outputs = ports.iter().filter(|p| p.output).count();
        match ports.first() {
            Some(p) if p.output => {}
            Some(_) if ansi || ports.iter().all(|p| p.input || p.output) => {
                self.err(n, "the first port in a primitive must be an output port".into())
            }
            _ => {}
        }
        if outputs > 1 {
            self.err(n, "primitives must have exactly one output port".into());
        }
        let inputs = ports.iter().filter(|p| p.input).count();
        if inputs == 0 && !ports.is_empty() {
            self.err(n, "primitives must have at least one input port".into());
        }
        let sequential = ports.iter().any(|p| p.output && p.reg);
        if let Some((c, name, value)) = initial_target {
            let out_port = ports.iter().find(|p| p.output);
            if !sequential {
                self.err(c, "initial statements are only allowed in sequential primitives".into());
            } else if !out_port.is_some_and(|p| p.name == name) {
                self.err(c, format!("initial statement target '{name}' is not the primitive's output"));
            } else if out_port.is_some_and(|p| p.init) {
                self.err(c, "the primitive's output already has an initial value".into());
            }
            let v = value.concat().to_ascii_lowercase();
            if !matches!(
                v.as_str(),
                "0" | "1" | "1'b0" | "1'b1" | "1'bx" | "'b0" | "'b1" | "'bx" | "1'h0" | "1'h1" | "1'hx"
            ) {
                self.err(c, format!("initial value '{}' of a primitive must be 0, 1 or x", value.concat()));
            }
        }
        if let Some(tb) = table {
            self.table(tb, inputs.max(1), sequential);
        }
    }

    fn table(&mut self, tb: NodeId, inputs: usize, sequential: bool) {
        let body: String = text(self.t, tb).iter().filter(|x| *x != "table" && *x != "endtable").cloned().collect();
        let mut rows: Vec<(Vec<Field>, u8)> = Vec::new();
        let body = body.trim_end();
        if !body.is_empty() && !body.ends_with(';') {
            self.err(tb, "expected ';' after a primitive table row".into());
        }
        for row in body.split(';').filter(|r| !r.is_empty()) {
            let parts: Vec<&str> = row.split(':').collect();
            let want = if sequential { 3 } else { 2 };
            if parts.len() != want {
                self.err(tb, format!("a primitive table row must have {want} fields separated by ':'"));
                continue;
            }
            let Some(fields) = self.input_fields(tb, parts[0], inputs, sequential) else { continue };
            let mut fields = fields;
            if sequential {
                let s = parts[1].as_bytes();
                match (s.len(), s.first().and_then(|&c| levels(c))) {
                    (1, Some(l)) => fields.push(Field::Levels(l)),
                    _ => {
                        self.err(tb, format!("invalid symbol '{}' in state table", parts[1]));
                        continue;
                    }
                }
            }
            let o = parts[want - 1].as_bytes();
            let ok = o.len() == 1
                && (matches!(o[0].to_ascii_lowercase(), b'0' | b'1' | b'x') || (o[0] == b'-' && sequential));
            if !ok {
                let msg = if o == b"-" {
                    "'-' is only allowed in the output field of a sequential primitive".to_string()
                } else {
                    format!("invalid symbol '{}' in state table", parts[want - 1])
                };
                self.err(tb, msg);
                continue;
            }
            let out = o[0].to_ascii_lowercase();
            // `-` keeps the current state: a row with it agrees with any
            // value its own state field allows (as slang decides).
            let state_of = |f: &Vec<Field>| match f.last() {
                Some(Field::Levels(l)) if sequential => l.clone(),
                _ => Vec::new(),
            };
            let keeps = |state: &[u8], o1: u8, o2: u8| o1 == b'-' && state.contains(&o2);
            let conflict = |f: &Vec<Field>, o2: u8| {
                if f.len() != fields.len() || !f.iter().zip(&fields).all(|(a, b)| overlaps(a, b)) {
                    return false;
                }
                o2 != out && !keeps(&state_of(f), o2, out) && !keeps(&state_of(&fields), out, o2)
            };
            let all_x = fields[..inputs].iter().all(|f| match f {
                Field::Levels(l) => l == b"x",
                Field::Edges(e) => e.iter().all(|&(a, b)| a == b'x' && b == b'x'),
            });
            if all_x && out != b'x' && !rows.iter().any(|(f, o2)| conflict(f, *o2) || f == &fields) {
                self.err(tb, "a primitive table row with all 'x' inputs must have an 'x' output".into());
            }
            if rows.iter().any(|(f, o2)| conflict(f, *o2)) {
                self.err(
                    tb,
                    "primitive table row duplicates a set of inputs with a different specified output value".into(),
                );
            }
            rows.push((fields, out));
        }
    }

    /// The input symbols of a row, or `None` (with an error).
    fn input_fields(&mut self, tb: NodeId, s: &str, inputs: usize, sequential: bool) -> Option<Vec<Field>> {
        let b = s.as_bytes();
        let mut fields = Vec::new();
        let mut i = 0;
        let mut edge = false;
        while i < b.len() {
            let c = b[i];
            let f = if c == b'(' {
                let (Some(&v), Some(&w), Some(&b')')) = (b.get(i + 1), b.get(i + 2), b.get(i + 3)) else {
                    self.err(tb, format!("invalid edge transition specifier in '{s}'"));
                    return None;
                };
                let (Some(lv), Some(lw)) = (levels(v), levels(w)) else {
                    let bad = if levels(v).is_none() { v } else { w } as char;
                    self.err(tb, format!("invalid symbol '{bad}' in edge transition specifier"));
                    return None;
                };
                if lv.len() == 1 && lv == lw {
                    self.err(tb, "edge transition must be between two different values".into());
                    return None;
                }
                i += 4;
                Field::Edges(edges_of(&lv, &lw))
            } else if let Some(l) = levels(c) {
                i += 1;
                Field::Levels(l)
            } else if let Some(e) = edge_symbol(c) {
                i += 1;
                Field::Edges(e)
            } else {
                self.err(tb, format!("invalid symbol '{}' in state table", c as char));
                return None;
            };
            if matches!(f, Field::Edges(_)) {
                if !sequential {
                    self.err(tb, "edge transition can only be used in a sequential primitive -- are you missing a 'reg' specifier on the output port?".into());
                    return None;
                }
                if edge {
                    self.err(tb, "a primitive table row can have at most one edge transition".into());
                    return None;
                }
                edge = true;
            }
            fields.push(f);
        }
        if fields.len() != inputs {
            self.err(tb, format!("expected {inputs} input symbols in a primitive table row, found {}", fields.len()));
            return None;
        }
        Some(fields)
    }
}
