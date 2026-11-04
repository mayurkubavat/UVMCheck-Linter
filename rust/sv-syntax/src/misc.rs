//! Slice 4: the rarer items. Attributes, nettype, specify blocks and
//! specparams, user-defined primitives, configs and randsequence.

use crate::blocks::port_list;
use crate::expr::{arg_list, expr, expr_bp, Flags};
use crate::grammar::{
    data_type, dim, end_label, group, name_token, param_decl, paren_expr, statement, to_semi, END_KEYWORDS,
};
use crate::parser::Parser;
use crate::SyntaxKind::{self, *};

/// Binding power that keeps an operand to a primary with its selects,
/// calls and members: no binary operator binds this tightly.
const PRIMARY_BP: u8 = 30;
/// Stops an expression in front of `&&&` (timing check conditions).
const BEFORE_MATCH_BP: u8 = 8;

fn at_attr_end(p: &Parser) -> bool {
    p.at("*") && p.nth(1) == ")" && p.joined(1)
}

/// `(* name [= expr] {, name [= expr]} *)`, any number of them.
pub(crate) fn attributes(p: &mut Parser) {
    while p.at("(") && p.nth(1) == "*" && p.joined(1) && !(p.nth(2) == ")" && p.joined(2)) {
        let m = p.start();
        p.bump();
        p.bump();
        while !p.at_end() && !at_attr_end(p) {
            let before = p.pos();
            let s = p.start();
            name_token(p);
            if p.eat("=") {
                expr(p);
            }
            s.complete(p, AttrSpec);
            if !p.eat(",") {
                if !at_attr_end(p) {
                    p.error("expected `*)`".into());
                    // Skip to the closer so the rest of the file still parses.
                    while !p.at_end() && !at_attr_end(p) && !p.at(";") && !p.at_any(END_KEYWORDS) {
                        p.bump();
                    }
                }
                if p.pos() == before {
                    break;
                }
                if !at_attr_end(p) {
                    break;
                }
            }
        }
        if at_attr_end(p) {
            p.bump();
            p.bump();
        }
        m.complete(p, Attribute);
    }
}

/// True at the `*` of a `*)` closing an attribute: ends any expression.
pub(crate) fn ends_attribute(p: &Parser) -> bool {
    at_attr_end(p)
}

/// `nettype data_type name [with tf];`
pub(crate) fn nettype(p: &mut Parser) -> SyntaxKind {
    p.bump();
    data_type(p);
    name_token(p);
    if p.eat("with") {
        expr_bp(p, PRIMARY_BP, Flags::default());
    }
    p.expect(";");
    NetTypeDecl
}

/// `alias a = b {= c};`
pub(crate) fn net_alias(p: &mut Parser) -> SyntaxKind {
    p.bump();
    loop {
        expr(p);
        if !p.eat("=") {
            break;
        }
    }
    p.expect(";");
    NetAlias
}

/// `e [: e : e]`
fn min_typ_max(p: &mut Parser) {
    expr(p);
    if p.at(":") {
        p.bump();
        expr(p);
        p.expect(":");
        expr(p);
    }
}

/// `specparam [range] name = value {, ...};`
pub(crate) fn specparam(p: &mut Parser) -> SyntaxKind {
    p.bump();
    if p.at("[") {
        dim(p);
    }
    loop {
        let d = p.start();
        let pulse = p.cur().starts_with("PATHPULSE$");
        name_token(p);
        p.expect("=");
        if pulse && p.at("(") {
            // `PATHPULSE$ = (reject [, error])`
            p.bump();
            min_typ_max(p);
            if p.eat(",") {
                min_typ_max(p);
            }
            p.expect(")");
        } else {
            min_typ_max(p);
        }
        d.complete(p, Declarator);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
    SpecparamDecl
}

/// `specify { item } endspecify`
pub(crate) fn specify(p: &mut Parser) -> SyntaxKind {
    p.bump();
    while !p.at_end() && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        specify_item(p);
        if p.pos() == before {
            p.bump();
        }
    }
    p.expect("endspecify");
    SpecifyBlock
}

fn specify_item(p: &mut Parser) {
    let m = p.start();
    let k = match p.cur() {
        "specparam" => specparam(p),
        "pulsestyle_onevent" | "pulsestyle_ondetect" | "showcancelled" | "noshowcancelled" => {
            to_semi(p);
            SpecifyOther
        }
        "if" => {
            p.bump();
            paren_expr(p);
            path_decl(p)
        }
        "ifnone" => {
            p.bump();
            path_decl(p)
        }
        "(" => path_decl(p),
        s if s.starts_with('$') => timing_check(p),
        s => {
            p.error(format!("unexpected `{s}` in specify block"));
            to_semi(p);
            SpecifyOther
        }
    };
    m.complete(p, k);
}

fn path_terminals(p: &mut Parser) {
    loop {
        expr_bp(p, PRIMARY_BP, Flags::default());
        if !p.eat(",") {
            break;
        }
    }
}

/// `( [edge] src [polarity] =>|*> dst ) = delays ;`; the destination of an
/// edge-sensitive path is `( dst [polarity]: data )`.
fn path_decl(p: &mut Parser) -> SyntaxKind {
    let s = p.start();
    p.expect("(");
    if p.at_any(&["posedge", "negedge", "edge"]) {
        p.bump();
    }
    path_terminals(p);
    if p.at_any(&["+", "-"]) && matches!(p.nth(1), "=>" | "*>") {
        p.bump();
    }
    if p.at_any(&["+=", "-="]) && p.nth(1) == ">" && p.joined(1) {
        p.bump();
        p.bump();
    } else if p.at_any(&["=>", "*>"]) {
        p.bump();
    } else {
        p.error(format!("expected `=>` or `*>`, found `{}`", p.cur()));
    }
    if p.at("(") {
        p.bump();
        path_terminals(p);
        if p.at_any(&["+:", "-:", ":"]) {
            p.bump();
        }
        expr(p);
        p.expect(")");
    } else {
        path_terminals(p);
    }
    p.expect(")");
    s.complete(p, PathSpec);
    p.expect("=");
    let d = p.start();
    if p.at("(") {
        p.bump();
        loop {
            min_typ_max(p);
            if !p.eat(",") {
                break;
            }
        }
        p.expect(")");
    } else {
        min_typ_max(p);
    }
    d.complete(p, PathDelays);
    p.expect(";");
    PathDecl
}

/// `$setup(data, posedge clk &&& en, 10, notifier);`; arguments may be
/// empty.
fn timing_check(p: &mut Parser) -> SyntaxKind {
    p.bump();
    p.expect("(");
    loop {
        let a = p.start();
        if !p.at_any(&[",", ")"]) {
            if p.at_any(&["posedge", "negedge", "edge"]) {
                p.bump();
                if p.at("[") {
                    group(p);
                }
            }
            expr_bp(p, BEFORE_MATCH_BP, Flags::default());
            if p.at(":") {
                p.bump();
                expr(p);
                p.expect(":");
                expr(p);
            }
            if p.eat("&&&") {
                expr(p);
            }
        }
        a.complete(p, TimingCheckArg);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(")");
    p.expect(";");
    TimingCheck
}

/// `primitive name (ports); decls [initial q = v;] table ... endtable
/// endprimitive`
pub(crate) fn udp(p: &mut Parser) -> SyntaxKind {
    p.bump();
    name_token(p);
    if p.at("(") {
        let m = p.start();
        group(p);
        m.complete(p, ParenList);
    }
    p.expect(";");
    while !p.at_end() && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        let m = p.start();
        crate::grammar::attributes(p);
        let k = match p.cur() {
            "output" | "input" | "reg" => {
                to_semi(p);
                UdpPortDecl
            }
            "initial" => {
                p.bump();
                name_token(p);
                p.expect("=");
                expr(p);
                p.expect(";");
                UdpInitial
            }
            "table" => {
                p.bump();
                while !p.at_end() && !p.at_any(END_KEYWORDS) {
                    p.bump();
                }
                p.expect("endtable");
                UdpTable
            }
            s => {
                p.error(format!("unexpected `{s}` in primitive"));
                to_semi(p);
                SpecifyOther
            }
        };
        m.complete(p, k);
        if p.pos() == before {
            p.bump();
        }
    }
    p.expect("endprimitive");
    end_label(p);
    UdpDecl
}

/// `config name; rules endconfig`; local parameters are parsed, the
/// library rules are kept as written.
pub(crate) fn config(p: &mut Parser) -> SyntaxKind {
    p.bump();
    name_token(p);
    p.expect(";");
    while !p.at_end() && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        let m = p.start();
        if p.at("localparam") {
            param_decl(p);
            m.complete(p, ParamDecl);
        } else {
            to_semi(p);
            m.complete(p, ConfigRule);
        }
        if p.pos() == before {
            p.bump();
        }
    }
    p.expect("endconfig");
    end_label(p);
    ConfigDecl
}

// ---- randsequence -----------------------------------------------------

/// `randsequence ([name]) productions endsequence`
pub(crate) fn randsequence(p: &mut Parser) -> SyntaxKind {
    p.bump();
    p.expect("(");
    if !p.at(")") {
        name_token(p);
    }
    p.expect(")");
    while !p.at_end() && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        production(p);
        if p.pos() == before {
            p.error(format!("unexpected `{}` in randsequence", p.cur()));
            p.bump();
        }
    }
    p.expect("endsequence");
    RandSequenceStmt
}

/// `[data_type | void] name [(ports)] : rule {| rule} ;`
fn production(p: &mut Parser) {
    let m = p.start();
    if !(p.at_ident() && matches!(p.nth(1), ":" | "(")) {
        if p.at("void") {
            p.bump();
        } else {
            data_type(p);
        }
    }
    name_token(p);
    if p.at("(") {
        port_list(p);
    }
    p.expect(":");
    loop {
        rs_rule(p);
        if !p.eat("|") {
            break;
        }
    }
    p.expect(";");
    m.complete(p, Production);
}

/// `[rand join [(expr)]] items [:= weight [code]]`
fn rs_rule(p: &mut Parser) {
    let m = p.start();
    if p.at("rand") && p.nth(1) == "join" {
        let j = p.start();
        p.bump();
        p.bump();
        if p.at("(") {
            p.bump();
            expr(p);
            p.expect(")");
        }
        j.complete(p, RsRandJoin);
    }
    while !p.at_end() && !p.at_any(&[";", "|", ":="]) && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        rs_item(p);
        if p.pos() == before {
            break;
        }
    }
    if p.at(":=") {
        let w = p.start();
        p.bump();
        expr_bp(p, PRIMARY_BP, Flags::default());
        if p.at("{") {
            code_block(p);
        }
        w.complete(p, RsWeight);
    }
    m.complete(p, RsRule);
}

fn rs_item(p: &mut Parser) {
    if p.at("{") {
        code_block(p);
        return;
    }
    let m = p.start();
    let k = match p.cur() {
        "if" => {
            p.bump();
            paren_expr(p);
            rs_item(p);
            if p.eat("else") {
                rs_item(p);
            }
            RsIfElse
        }
        "repeat" => {
            p.bump();
            paren_expr(p);
            rs_item(p);
            RsRepeat
        }
        "case" => {
            p.bump();
            paren_expr(p);
            while !p.at_end() && !p.at_any(END_KEYWORDS) {
                let before = p.pos();
                let c = p.start();
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
                rs_item(p);
                p.expect(";");
                c.complete(p, RsCaseItem);
                if p.pos() == before {
                    p.bump();
                }
            }
            p.expect("endcase");
            RsCase
        }
        _ => {
            if !p.at_ident() {
                m.abandon(p);
                return;
            }
            name_token(p);
            if p.at("(") {
                arg_list(p);
            }
            RsProdItem
        }
    };
    m.complete(p, k);
}

/// `{ declarations and statements }`
fn code_block(p: &mut Parser) {
    let m = p.start();
    p.bump();
    while !p.at_end() && !p.at("}") {
        if !statement(p) {
            break;
        }
    }
    p.expect("}");
    m.complete(p, RsCodeBlock);
}
