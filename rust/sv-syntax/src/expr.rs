//! Expressions: precedence climbing over the operator table of IEEE
//! 1800-2023 Table 11-2, with primaries, selects, calls, casts,
//! concatenations, assignment patterns, `new` and `with` clauses.

use sv_lexer::Kind;

use crate::blocks::{constraint_block, timing_control};
use crate::grammar::{attributes, data_type, BUILTIN_TYPES};
use crate::parser::{Completed, Parser};
use crate::SyntaxKind::*;

const ASSIGN_OPS: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

/// Binding powers (left, right) of a binary operator; right-associative
/// operators have equal powers.
fn infix(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "**" => (28, 29),
        "*" | "/" | "%" => (26, 27),
        "+" | "-" => (24, 25),
        "<<" | ">>" | "<<<" | ">>>" => (22, 23),
        "<" | "<=" | ">" | ">=" | "inside" => (20, 21),
        "==" | "!=" | "===" | "!==" | "==?" | "!=?" => (18, 19),
        "&" => (16, 17),
        "^" | "~^" | "^~" => (14, 15),
        "|" => (12, 13),
        "&&" => (10, 11),
        "||" => (8, 9),
        "->" | "<->" => (4, 4),
        _ => return None,
    })
}

const COND_BP: u8 = 6;
const MATCH_BP: u8 = 7;
const ASSIGN_BP: u8 = 2;

#[derive(Clone, Copy, Default)]
pub(crate) struct Flags {
    /// `=` and friends may follow the first operand (inside parentheses,
    /// for-loop headers, statements).
    pub assign: bool,
    /// `<=` after the first operand is a nonblocking assignment.
    pub nonblocking: bool,
    /// `->` ends the expression (constraint implication).
    pub no_implication: bool,
}

pub(crate) const ASSIGNABLE: Flags = Flags { assign: true, nonblocking: false, no_implication: false };
pub(crate) const STATEMENT: Flags = Flags { assign: true, nonblocking: true, no_implication: false };

/// An expression; false (with an error) if none starts here.
pub(crate) fn expr(p: &mut Parser) -> bool {
    expr_bp(p, 0, Flags::default()).is_some()
}

pub(crate) fn expr_with(p: &mut Parser, flags: Flags) -> bool {
    expr_bp(p, 0, flags).is_some()
}

pub(crate) fn expr_bp(p: &mut Parser, min_bp: u8, flags: Flags) -> Option<Completed> {
    let mut lhs = unary(p)?;
    loop {
        let op = p.cur();
        if flags.assign && (ASSIGN_OPS.contains(&op) || (flags.nonblocking && op == "<=")) && ASSIGN_BP >= min_bp {
            let m = lhs.precede(p);
            p.bump();
            if p.at_any(&["#", "@", "##", "repeat"]) {
                // Intra-assignment timing: `a = #1 b`, `a <= repeat (2) @e b`.
                let t = p.start();
                timing_control(p);
                expr_bp(p, ASSIGN_BP, Flags::default());
                t.complete(p, TimedExpr);
            } else {
                expr_bp(p, ASSIGN_BP, Flags::default());
            }
            lhs = m.complete(p, BinaryExpr);
            continue;
        }
        if op == "?" {
            if COND_BP < min_bp {
                break;
            }
            let m = lhs.precede(p);
            p.bump();
            attributes(p);
            expr_bp(p, 0, Flags::default());
            p.expect(":");
            expr_bp(p, COND_BP, Flags::default());
            lhs = m.complete(p, CondExpr);
            continue;
        }
        if op == "matches" || op == "&&&" {
            // Pattern match and guard in a condition: not operators.
            if MATCH_BP < min_bp {
                break;
            }
            let m = lhs.precede(p);
            p.bump();
            if op == "matches" {
                pattern(p);
            } else {
                expr_bp(p, MATCH_BP + 1, Flags::default());
            }
            lhs = m.complete(p, MatchExpr);
            continue;
        }
        if crate::misc::ends_attribute(p) {
            break;
        }
        let Some((l, r)) = infix(op) else { break };
        if l < min_bp || (flags.no_implication && op == "->") {
            break;
        }
        let m = lhs.precede(p);
        p.bump();
        attributes(p);
        if op == "inside" {
            range_list(p);
            lhs = m.complete(p, InsideExpr);
        } else {
            expr_bp(p, r, Flags::default());
            lhs = m.complete(p, BinaryExpr);
        }
    }
    Some(lhs)
}

const PREFIX: &[&str] = &["+", "-", "!", "~", "&", "~&", "|", "~|", "^", "~^", "^~", "++", "--"];

fn unary(p: &mut Parser) -> Option<Completed> {
    if p.at_any(PREFIX) {
        let m = p.start();
        p.bump();
        attributes(p);
        if unary(p).is_none() {
            m.complete(p, PrefixExpr);
            return None;
        }
        return Some(m.complete(p, PrefixExpr));
    }
    let prim = primary(p)?;
    Some(postfix(p, prim))
}

/// `{ value_range, ... }` after `inside`: expressions or `[lo : hi]`.
pub(crate) fn range_list(p: &mut Parser) {
    let m = p.start();
    if !p.expect("{") {
        m.complete(p, RangeList);
        return;
    }
    while !p.at_end() && !p.at("}") {
        value_range(p);
        if !p.eat(",") {
            break;
        }
    }
    p.expect("}");
    m.complete(p, RangeList);
}

/// An expression or `[lo : hi]` (also `[a +/- b]`, `[a +%- b]`).
pub(crate) fn value_range(p: &mut Parser) {
    if p.at("[") {
        let m = p.start();
        p.bump();
        expr(p);
        if p.at_any(&[":", "+/-", "+%-"]) {
            p.bump();
            expr(p);
        }
        p.expect("]");
        m.complete(p, OpenRange);
    } else {
        expr(p);
    }
}

fn is_literal(k: Option<Kind>) -> bool {
    matches!(
        k,
        Some(
            Kind::IntegerLiteral
                | Kind::IntegerBase
                | Kind::UnbasedUnsizedLiteral
                | Kind::RealLiteral
                | Kind::TimeLiteral
                | Kind::StringLiteral
                | Kind::TripleQuotedString
        )
    )
}

/// Keywords that can start a cast `type'(...)`.
const CAST_KEYWORDS: &[&str] = &[
    "bit", "logic", "reg", "byte", "shortint", "int", "longint", "integer", "time", "real", "shortreal", "realtime",
    "string", "signed", "unsigned", "void", "const", "chandle", "event",
];

/// The digits after a base: lexed as such, or made by a macro (`32'h`ADDR`,
/// whose expansion lexes as a number or a name of hex digits).
/// `1A11_0000` from a macro lexes as `1` and `A11_0000`: take the touching pieces.
fn based_digits(p: &mut Parser) {
    let digits = |p: &Parser| match p.nth_kind(0) {
        Some(Kind::IntegerLiteral) | Some(Kind::Identifier) => {
            p.nth(0).bytes().all(|c| c.is_ascii_hexdigit() || matches!(c, b'_' | b'x' | b'X' | b'z' | b'Z' | b'?'))
        }
        _ => false,
    };
    if p.nth_kind(0) == Some(Kind::VectorDigits) {
        p.bump();
    } else if digits(p) {
        p.bump();
        while digits(p) && !p.space_before(0) {
            p.bump();
        }
    }
}

fn primary(p: &mut Parser) -> Option<Completed> {
    let m = p.start();
    let k = p.nth_kind(0);
    match p.cur() {
        _ if k == Some(Kind::IntegerLiteral) => {
            p.bump();
            if p.nth_kind(0) == Some(Kind::IntegerBase) {
                p.bump();
                based_digits(p);
            } else if p.at("'") && p.nth(1) == "(" {
                // Size cast: `8'(x)`.
                let c = m.complete(p, Literal).precede(p);
                p.bump();
                paren_expr_body(p);
                return Some(c.complete(p, CastExpr));
            }
            Some(m.complete(p, Literal))
        }
        _ if k == Some(Kind::IntegerBase) => {
            p.bump();
            based_digits(p);
            Some(m.complete(p, Literal))
        }
        _ if is_literal(k) => {
            p.bump();
            Some(m.complete(p, Literal))
        }
        _ if k == Some(Kind::Identifier) || k == Some(Kind::EscapedIdentifier) || k == Some(Kind::SystemIdentifier) => {
            p.bump();
            if p.at("#") && p.nth(1) == "(" {
                param_values(p);
            }
            Some(m.complete(p, NameExpr))
        }
        "this" | "super" | "null" | "$" | "local" | "$unit" | "$root" | "std" => {
            p.bump();
            Some(m.complete(p, NameExpr))
        }
        "(" => {
            p.bump();
            expr_with(p, ASSIGNABLE);
            if p.eat(":") {
                // min:typ:max
                expr(p);
                p.expect(":");
                expr(p);
            }
            p.expect(")");
            Some(m.complete(p, ParenExpr))
        }
        "{" => {
            concatenation(p);
            Some(m.complete(p, ConcatExpr))
        }
        "'{" => {
            assignment_pattern(p);
            Some(m.complete(p, AssignPattern))
        }
        "new" => {
            p.bump();
            if p.at("[") {
                p.bump();
                expr(p);
                p.expect("]");
                if p.at("(") {
                    p.bump();
                    expr(p);
                    p.expect(")");
                }
            } else if p.at("(") {
                arg_list(p);
            } else if p.at_ident() || p.at_any(&["this", "super"]) {
                // Shallow copy: `new src`.
                unary(p);
            }
            Some(m.complete(p, NewExpr))
        }
        "type" => {
            p.bump();
            p.expect("(");
            type_or_expr(p);
            p.expect(")");
            Some(m.complete(p, NameExpr))
        }
        "tagged" => {
            p.bump();
            if p.at_ident() {
                p.bump();
            }
            if !p.at_any(&[")", ";", ",", "}", ":"]) {
                unary(p);
            }
            Some(m.complete(p, TaggedExpr))
        }
        "#" | "@" | "##" => {
            timing_control(p);
            expr(p);
            Some(m.complete(p, TimedExpr))
        }
        s if CAST_KEYWORDS.contains(&s) && p.nth(1) == "'" => {
            p.bump();
            let c = m.complete(p, NameExpr);
            Some(c)
        }
        s if BUILTIN_TYPES.contains(&s) => {
            // A type where an expression may stand: `$bits(int)`.
            data_type(p);
            Some(m.complete(p, TypeExpr))
        }
        _ => {
            p.error(format!("expected an expression, found `{}`", p.cur()));
            m.abandon(p);
            None
        }
    }
}

/// Member access, selects, calls, scope, casts, `with`, postfix ++/--.
fn postfix(p: &mut Parser, mut lhs: Completed) -> Completed {
    loop {
        match p.cur() {
            "." => {
                let m = lhs.precede(p);
                p.bump();
                if p.at_ident() || p.nth_kind(0) == Some(Kind::Keyword) {
                    p.bump();
                } else {
                    p.error("expected a member name".into());
                }
                lhs = m.complete(p, MemberExpr);
            }
            "::" => {
                let m = lhs.precede(p);
                p.bump();
                if p.at_ident() || p.nth_kind(0) == Some(Kind::Keyword) {
                    p.bump();
                    if p.at("#") && p.nth(1) == "(" {
                        param_values(p);
                    }
                } else {
                    p.error("expected a name after `::`".into());
                }
                lhs = m.complete(p, ScopedExpr);
            }
            // `a[*2]`, `a[->1]`, `a[=3]`: a repetition, not a select.
            "[" if matches!(p.nth(1), "*" | "->" | "=") => return lhs,
            "[" => {
                let m = lhs.precede(p);
                p.bump();
                expr(p);
                if p.at_any(&[":", "+:", "-:"]) {
                    p.bump();
                    expr(p);
                }
                p.expect("]");
                lhs = m.complete(p, SelectExpr);
            }
            "(" if p.nth(1) == "*" && p.joined(1) && p.nth(2) != ")" => {
                // `f (* attr *) (args)`: attributes on a call.
                let mut i = 2;
                while !p.nth(i).is_empty() && !(p.nth(i) == "*" && p.nth(i + 1) == ")" && p.joined(i + 1)) {
                    i += 1;
                }
                if p.nth(i + 2) != "(" {
                    return lhs;
                }
                let m = lhs.precede(p);
                attributes(p);
                arg_list(p);
                lhs = m.complete(p, CallExpr);
            }
            "(" => {
                let m = lhs.precede(p);
                arg_list(p);
                lhs = m.complete(p, CallExpr);
            }
            "'" if p.nth(1) == "(" => {
                let m = lhs.precede(p);
                p.bump();
                paren_expr_body(p);
                lhs = m.complete(p, CastExpr);
            }
            "'{" => {
                let m = lhs.precede(p);
                let a = p.start();
                assignment_pattern(p);
                a.complete(p, AssignPattern);
                lhs = m.complete(p, CastExpr);
            }
            "with" => {
                let m = lhs.precede(p);
                let w = p.start();
                p.bump();
                if p.at("(") {
                    p.bump();
                    if !p.at(")") {
                        expr(p);
                        while p.eat(",") {
                            expr(p);
                        }
                    }
                    p.expect(")");
                }
                if p.at("{") {
                    constraint_block(p);
                }
                w.complete(p, WithClause);
                lhs = m.complete(p, CallExpr);
            }
            "++" | "--" => {
                let m = lhs.precede(p);
                p.bump();
                lhs = m.complete(p, PostfixExpr);
            }
            _ => return lhs,
        }
    }
}

/// `( expr )` after a cast tick.
fn paren_expr_body(p: &mut Parser) {
    p.expect("(");
    expr_with(p, ASSIGNABLE);
    p.expect(")");
}

/// A type or an expression: arguments of `$bits`, parameter values.
pub(crate) fn type_or_expr(p: &mut Parser) {
    if (BUILTIN_TYPES.contains(&p.cur()) && p.nth(1) != "'") || p.at("virtual") {
        data_type(p);
    } else {
        expr(p);
    }
}

/// `( arg, .name(arg), , ... )`
pub(crate) fn arg_list(p: &mut Parser) {
    let m = p.start();
    p.expect("(");
    while !p.at_end() && !p.at(")") {
        if p.at(",") {
            p.bump();
            continue;
        }
        let a = p.start();
        // `(* async *) .port(sig)`
        crate::misc::attributes(p);
        if p.at(".") && p.nth(1) == "*" {
            p.bump();
            p.bump();
        } else if p.at(".") {
            p.bump();
            if p.at_ident() || p.nth_kind(0) == Some(Kind::Keyword) {
                p.bump();
            }
            if p.at("(") {
                p.bump();
                if !p.at(")") {
                    arg_value(p);
                }
                p.expect(")");
            }
        } else {
            let before = p.pos();
            arg_value(p);
            if p.pos() == before {
                a.complete(p, Arg);
                // Recover: skip to the next separator.
                while !p.at_end() && !p.at_any(&[",", ")", ";"]) {
                    p.bump();
                }
                continue;
            }
        }
        a.complete(p, Arg);
        if !p.at(")") && !p.expect(",") {
            break;
        }
    }
    p.expect(")");
    m.complete(p, ArgList);
}

/// An argument; checker and sequence actuals may be edge events.
fn arg_value(p: &mut Parser) {
    if p.at_any(&["posedge", "negedge", "edge"]) {
        p.bump();
    }
    type_or_expr(p);
}

/// `#( values )` on a class or module name.
pub(crate) fn param_values(p: &mut Parser) {
    let m = p.start();
    p.bump();
    arg_list(p);
    m.complete(p, ParamList);
}

/// `{ a, b }`, `{ n { a } }`, `{ << 8 { a } }`, `{}`.
fn concatenation(p: &mut Parser) {
    p.bump();
    if p.eat("}") {
        return;
    }
    if p.at_any(&["<<", ">>"]) {
        p.bump();
        if !p.at("{") {
            type_or_expr(p);
        }
        p.expect("{");
        if !p.at("}") {
            stream_item(p);
            while p.eat(",") {
                stream_item(p);
            }
        }
        p.expect("}");
        p.expect("}");
        return;
    }
    expr(p);
    if p.at("{") {
        // Replication: `{n{...}}`.
        let r = p.start();
        concatenation(p);
        r.complete(p, ConcatExpr);
        p.expect("}");
        return;
    }
    while p.eat(",") {
        expr(p);
    }
    p.expect("}");
}

fn stream_item(p: &mut Parser) {
    expr(p);
    if p.at("with") {
        p.bump();
        p.expect("[");
        expr(p);
        if p.at_any(&[":", "+:", "-:"]) {
            p.bump();
            expr(p);
        }
        p.expect("]");
    }
}

/// `'{ a, b }`, `'{ key: value, default: v }`, `'{ n { a } }`.
fn assignment_pattern(p: &mut Parser) {
    p.bump();
    if p.eat("}") {
        return;
    }
    loop {
        if p.at("default") {
            p.bump();
        } else {
            type_or_expr(p);
        }
        if p.at("{") {
            // Replication.
            let r = p.start();
            concatenation(p);
            r.complete(p, ConcatExpr);
        } else if p.eat(":") {
            expr(p);
        }
        if !p.eat(",") {
            break;
        }
    }
    p.expect("}");
}

/// A pattern after `matches`: `.name`, `.*`, `tagged T [pattern]`,
/// `'{ [name :] pattern, ... }` or a constant expression.
pub(crate) fn pattern(p: &mut Parser) {
    let m = p.start();
    if p.at(".") {
        p.bump();
        if !p.eat("*") {
            p.bump();
        }
    } else if p.at("tagged") {
        p.bump();
        p.bump();
        if !p.at_any(&[")", ",", "}", ":", "?", "&&&", ";"]) {
            pattern(p);
        }
    } else if p.at("'{") {
        p.bump();
        while !p.at_end() && !p.at("}") {
            if p.at_ident() && p.nth(1) == ":" {
                p.bump();
                p.bump();
            }
            pattern(p);
            if !p.eat(",") {
                break;
            }
        }
        p.expect("}");
    } else {
        expr_bp(p, MATCH_BP + 1, Flags::default());
    }
    m.complete(p, Pattern);
}
