//! Slice 3b: properties, sequences and concurrent assertions, clocking
//! blocks, `let`, `default disable iff`, DPI imports and exports, modports
//! and `bind`.
//!
//! Sequence and property operators build their own nodes; what sits
//! between them is either a nested sequence/property or a plain
//! expression, parsed by the expression parser (which stops at every
//! sequence operator, as none of them is an expression operator).

use crate::blocks::{port_list, timing_control};
use crate::expr::{arg_list, expr, expr_bp, expr_with, Flags, ASSIGNABLE};
use crate::grammar::{
    data_decl, end_label, is_decl_start, name_token, paren_expr, statement, subroutine, to_semi, END_KEYWORDS,
};
use crate::parser::Parser;
use crate::SyntaxKind::*;

/// Binary sequence and property operators.
const SEQ_BINARY: &[&str] = &[
    "and", "or", "intersect", "throughout", "within", "iff", "until", "s_until", "until_with", "s_until_with",
    "implies", "|->", "|=>", "#-#", "#=#",
];

/// Unary property operators, some with an optional `[range]`.
const PROP_UNARY: &[&str] =
    &["not", "nexttime", "s_nexttime", "always", "s_always", "eventually", "s_eventually"];

const ACCEPT_ON: &[&str] = &["accept_on", "reject_on", "sync_accept_on", "sync_reject_on"];

/// `[disable iff (expr)] property_expr`, after an optional clocking event.
pub(crate) fn property_spec(p: &mut Parser) {
    let m = p.start();
    if p.at("@") {
        timing_control(p);
    }
    if p.at("disable") && p.nth(1) == "iff" {
        let d = p.start();
        p.bump();
        p.bump();
        paren_expr(p);
        d.complete(p, DisableIff);
    }
    property_expr(p);
    m.complete(p, PropertyExpr);
}

/// A sequence or property expression: operands joined by sequence and
/// property operators and `##` delays.
pub(crate) fn property_expr(p: &mut Parser) {
    let m = p.start();
    loop {
        if p.at("##") {
            // A leading or chained cycle delay: `##1 b`, `a ##[1:3] b`.
            timing_control(p);
            continue;
        }
        if !operand(p) {
            break;
        }
        repetition(p);
        if p.at("##") {
            continue;
        }
        if p.at_any(SEQ_BINARY) {
            p.bump();
            continue;
        }
        break;
    }
    m.complete(p, SequenceExpr);
}

/// One operand; false if nothing could start here.
fn operand(p: &mut Parser) -> bool {
    let m = p.start();
    match p.cur() {
        "@" => {
            timing_control(p);
            property_expr(p);
        }
        s if PROP_UNARY.contains(&s) => {
            p.bump();
            if p.at("[") {
                cycle_range(p);
            }
            property_expr(p);
        }
        "posedge" | "negedge" | "edge" => {
            p.bump();
            if !operand(p) {
                p.error("expected an expression".into());
            }
        }
        "strong" | "weak" if p.nth(1) == "(" => {
            p.bump();
            p.bump();
            property_expr(p);
            p.expect(")");
        }
        "first_match" => {
            p.bump();
            p.expect("(");
            property_expr(p);
            while p.eat(",") {
                expr_with(p, ASSIGNABLE);
            }
            p.expect(")");
        }
        s if ACCEPT_ON.contains(&s) => {
            p.bump();
            paren_expr(p);
            property_expr(p);
        }
        "if" => {
            p.bump();
            paren_expr(p);
            property_expr(p);
            if p.eat("else") {
                property_expr(p);
            }
        }
        "case" => {
            p.bump();
            paren_expr(p);
            while !p.at_end() && !p.at("endcase") && !p.at_any(&END_KEYWORDS[2..]) {
                let before = p.pos();
                if p.eat("default") {
                    p.eat(":");
                } else {
                    loop {
                        expr(p);
                        if !p.eat(",") {
                            break;
                        }
                    }
                    p.expect(":");
                }
                property_expr(p);
                p.expect(";");
                if p.pos() == before {
                    p.bump();
                }
            }
            p.expect("endcase");
        }
        "(" if is_paren_sequence(p) => {
            p.bump();
            property_expr(p);
            // Match items: `(a, x = y, n++)`; an event list's `(e1, e2 iff c)`.
            while p.eat(",") {
                if arg_is_sequence(p) {
                    property_expr(p);
                } else {
                    expr_with(p, ASSIGNABLE);
                }
            }
            p.expect(")");
        }
        _ => {
            if expr_bp(p, 0, Flags::default()).is_none() {
                m.abandon(p);
                return false;
            }
            if p.at("dist") {
                crate::blocks::dist_list(p);
            }
        }
    }
    m.complete(p, SequenceExpr);
    true
}

/// Lookahead: an argument up to its `,` or `)` is a sequence, property
/// or event expression (a sequence or checker instance's actual).
pub(crate) fn arg_is_sequence(p: &Parser) -> bool {
    if p.at_any(&["posedge", "negedge", "edge", "@", "##"]) || p.at_any(PROP_UNARY) && p.nth(1) != "(" {
        return true;
    }
    let mut i = p.pos();
    let mut depth = 0i32;
    loop {
        let t = p.text_at(i);
        match t {
            "" | ";" => return false,
            "(" | "{" | "'{" => {
                if depth == 0
                    && t == "("
                    && !matches!(p.text_at(i.wrapping_sub(1)), "'" | "#")
                    && !matches!(p.kind_at(i.wrapping_sub(1)), Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::SystemIdentifier))
                    && paren_sequence_at(p, i, 0)
                {
                    return true;
                }
                depth += 1;
            }
            "[" => {
                if depth == 0 && (matches!(p.text_at(i + 1), "*" | "->" | "=") || p.text_at(i + 1) == "+" && p.text_at(i + 2) == "]") {
                    return true;
                }
                depth += 1;
            }
            ")" | "]" | "}" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            "," if depth == 0 => return false,
            "##" | "@" | "|->" | "|=>" | "#-#" | "#=#" | "dist" if depth == 0 => return true,
            _ if depth == 0 && SEQ_BINARY.contains(&t) && p.kind_at(i) == Some(sv_lexer::Kind::Keyword) => return true,
            _ => {}
        }
        i += 1;
    }
}

/// `[*n]`, `[*n:m]`, `[*]`, `[+]`, `[->n]`, `[=n:$]` after an operand.
fn repetition(p: &mut Parser) {
    while p.at("[") && matches!(p.nth(1), "*" | "->" | "=" | "+") {
        let m = p.start();
        p.bump();
        p.bump();
        if !p.at("]") {
            expr(p);
            if p.eat(":") && !p.eat("$") {
                expr(p);
            }
        }
        p.expect("]");
        m.complete(p, Repetition);
    }
}

/// `[n]`, `[n:m]`, `[n:$]` after `always`, `eventually`, `nexttime`.
fn cycle_range(p: &mut Parser) {
    let m = p.start();
    p.bump();
    expr(p);
    if p.eat(":") && !p.eat("$") {
        expr(p);
    }
    p.expect("]");
    m.complete(p, Repetition);
}

/// `(` that opens a sequence: some sequence operator, `##`, `@`, a
/// repetition or a match-item comma sits at its own level.
fn is_paren_sequence(p: &Parser) -> bool {
    paren_sequence_at(p, p.pos(), 0)
}

/// [`is_paren_sequence`] for the `(` at token `open`; a group nested in it
/// that is one makes it one too (`((a |=> b))`).
fn paren_sequence_at(p: &Parser, open: usize, nest: u32) -> bool {
    let mut i = open + 1;
    let mut depth = 0i32;
    loop {
        let t = p.text_at(i);
        match t {
            "" | ";" => return false,
            // Not a call's arguments.
            "(" if depth == 0
                && nest < 16
                && !matches!(p.kind_at(i - 1), Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::SystemIdentifier)) =>
            {
                if paren_sequence_at(p, i, nest + 1) {
                    return true;
                }
                depth += 1;
            }
            "(" | "{" | "'{" => depth += 1,
            "[" => {
                if depth == 0 && matches!(p.text_at(i + 1), "*" | "->" | "=" | "+") {
                    return true;
                }
                depth += 1;
            }
            ")" | "]" | "}" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            "##" | "," | "@" if depth == 0 => return true,
            _ if depth == 0
                && (SEQ_BINARY.contains(&t) || PROP_UNARY.contains(&t) || ACCEPT_ON.contains(&t))
                && p.kind_at(i) == Some(sv_lexer::Kind::Keyword) =>
            {
                return true
            }
            _ if depth == 0 && matches!(t, "|->" | "|=>" | "#-#" | "#=#") => return true,
            _ => {}
        }
        i += 1;
    }
}

/// `assert property (spec) action`, `cover sequence (...)`, `expect (...)`,
/// with the keyword at the cursor.
pub(crate) fn concurrent_assertion(p: &mut Parser) {
    concurrent_assertion_in(p, false);
}

/// [`concurrent_assertion`]; as a module item (`item`), an `else` after
/// an empty pass statement belongs to an enclosing generate `if`.
pub(crate) fn concurrent_assertion_in(p: &mut Parser, item: bool) {
    p.bump();
    p.eat("property");
    p.eat("sequence");
    p.expect("(");
    property_spec(p);
    p.expect(")");
    if !p.at("else") {
        let empty = p.at(";");
        if !statement(p) {
            p.error("expected a statement".into());
        }
        if empty && item {
            return;
        }
    }
    if p.at("else") {
        let e = p.start();
        p.bump();
        if !statement(p) {
            p.error("expected a statement".into());
        }
        e.complete(p, ElseClause);
    }
}

/// `property name [(ports)]; [decls] spec [;] endproperty` and the same
/// for `sequence ... endsequence`.
pub(crate) fn property_decl(p: &mut Parser, end_kw: &str) {
    p.bump();
    name_token(p);
    if p.at("(") {
        port_list(p);
    }
    p.expect(";");
    while is_decl_start(p) && !p.at_any(&["not", "strong", "weak"]) {
        let d = p.start();
        data_decl(p);
        d.complete(p, DataDecl);
    }
    if !p.at(end_kw) {
        property_spec(p);
    }
    while p.eat(";") {}
    p.expect(end_kw);
    end_label(p);
}

/// `[default|global] clocking [name] @(event); items endclocking`, or a
/// reference `default clocking name;`.
pub(crate) fn clocking(p: &mut Parser) {
    p.eat("default");
    p.eat("global");
    p.expect("clocking");
    if p.at_ident() {
        name_token(p);
    }
    if p.eat(";") {
        return;
    }
    if p.at_any(&["@", "@@"]) {
        timing_control(p);
    }
    p.expect(";");
    while !p.at_end() && !p.at("endclocking") && !p.at_any(&END_KEYWORDS[2..]) {
        let before = p.pos();
        clocking_item(p);
        if p.pos() == before {
            p.error(format!("unexpected `{}` in clocking block", p.cur()));
            p.bump();
        }
    }
    p.expect("endclocking");
    end_label(p);
}

fn clocking_item(p: &mut Parser) {
    let m = p.start();
    match p.cur() {
        "default" => {
            p.bump();
            skews(p);
            p.expect(";");
        }
        "input" | "output" | "inout" => {
            skews(p);
            loop {
                let d = p.start();
                name_token(p);
                if p.eat("=") {
                    expr(p);
                }
                d.complete(p, Declarator);
                if !p.eat(",") {
                    break;
                }
            }
            p.expect(";");
        }
        "property" => property_decl(p, "endproperty"),
        "sequence" => property_decl(p, "endsequence"),
        "let" => let_decl(p),
        _ => to_semi(p),
    }
    m.complete(p, ClockingItem);
}

/// `input [edge] [#d] output [edge] [#d]` skews.
fn skews(p: &mut Parser) {
    while p.at_any(&["input", "output", "inout"]) {
        p.bump();
        if p.at_any(&["posedge", "negedge", "edge"]) {
            p.bump();
        }
        if p.at("#") {
            timing_control(p);
        }
    }
}

/// `let name [(ports)] = expr;`
pub(crate) fn let_decl(p: &mut Parser) {
    p.bump();
    name_token(p);
    if p.at("(") {
        port_list(p);
    }
    p.expect("=");
    expr(p);
    p.expect(";");
}

/// `default disable iff expr;`
pub(crate) fn default_disable(p: &mut Parser) {
    p.bump();
    p.bump();
    p.expect("iff");
    expr(p);
    p.expect(";");
}

/// `import "DPI-C" [context|pure] [name =] function|task prototype;` and
/// `export "DPI-C" [name =] function|task name;`.
pub(crate) fn dpi(p: &mut Parser) {
    let export = p.at("export");
    p.bump();
    p.bump();
    if p.at_any(&["context", "pure"]) {
        p.bump();
    }
    if p.at_ident() && p.nth(1) == "=" {
        p.bump();
        p.bump();
    }
    if export {
        to_semi(p);
        return;
    }
    if p.at_any(&["function", "task"]) {
        // A prototype: same header as a method, no body.
        subroutine(p, 0, true);
    } else {
        to_semi(p);
    }
}

/// `modport name ( ports ), name ( ... );`
pub(crate) fn modport(p: &mut Parser) {
    p.bump();
    loop {
        let m = p.start();
        name_token(p);
        if p.eat("(") {
            while !p.at_end() && !p.at(")") {
                let before = p.pos();
                if p.at_any(&["input", "output", "inout", "ref", "import", "export", "clocking"]) {
                    p.bump();
                }
                if p.at_any(&["function", "task"]) {
                    subroutine(p, 0, true);
                } else if p.at(".") {
                    p.bump();
                    name_token(p);
                    p.expect("(");
                    if !p.at(")") {
                        expr(p);
                    }
                    p.expect(")");
                } else if p.at_ident() {
                    name_token(p);
                }
                if !p.eat(",") {
                    if p.pos() == before {
                        p.bump();
                        continue;
                    }
                    break;
                }
            }
            p.expect(")");
        }
        m.complete(p, Declarator);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}

/// `bind target [: inst, ...] module_type #(..) name (...);`
pub(crate) fn bind(p: &mut Parser) {
    p.bump();
    expr_bp(p, 30, Flags::default());
    if p.eat(":") {
        loop {
            expr_bp(p, 30, Flags::default());
            if !p.eat(",") {
                break;
            }
        }
    }
    let t = p.start();
    name_token(p);
    while p.at("::") {
        p.bump();
        name_token(p);
    }
    if p.at("#") && p.nth(1) == "(" {
        crate::expr::param_values(p);
    }
    t.complete(p, DataType);
    loop {
        let i = p.start();
        name_token(p);
        while p.at("[") {
            crate::grammar::dim(p);
        }
        if p.at("(") {
            arg_list(p);
        }
        i.complete(p, Instance);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}
