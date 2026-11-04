//! Slice 3a: headers (parameter and port lists, extends), timing and event
//! controls, constraint blocks and covergroups.

use crate::expr::{arg_list, expr, expr_bp, expr_with, range_list, value_range, Flags};
use crate::grammar::{
    attributes, data_type, dim, end_label, group, is_net_type, name_token, paren_expr, subroutine, to_semi,
    END_KEYWORDS,
};
use crate::parser::Parser;
use crate::SyntaxKind::*;

// ---- headers ----------------------------------------------------------

/// Module/interface/program/package header after the name, through `;`.
pub(crate) fn module_header(p: &mut Parser) {
    let h = p.start();
    while p.at("import") {
        let i = p.start();
        to_semi(p);
        i.complete(p, ImportDecl);
    }
    if p.at("#") {
        param_port_list(p);
    }
    if p.at("(") {
        port_list(p);
    }
    if !p.eat(";") {
        to_semi(p);
    }
    h.complete(p, Header);
}

/// Class header after the name: `#(params) extends T #(..) (args)
/// implements I, J ;`.
pub(crate) fn class_header(p: &mut Parser) {
    let h = p.start();
    if p.at("#") {
        param_port_list(p);
    }
    if p.at("extends") {
        let e = p.start();
        p.bump();
        data_type(p);
        if p.at("(") {
            if p.nth(1) == "default" {
                group(p);
            } else {
                arg_list(p);
            }
        }
        while p.eat(",") {
            // `interface class i extends a, b;`
            data_type(p);
        }
        e.complete(p, Extends);
    }
    if p.at("implements") {
        let e = p.start();
        p.bump();
        loop {
            data_type(p);
            if !p.eat(",") {
                break;
            }
        }
        e.complete(p, Extends);
    }
    if !p.eat(";") {
        to_semi(p);
    }
    h.complete(p, Header);
}

/// `#( [parameter] [type] name = value, type T = int, ... )`
pub(crate) fn param_port_list(p: &mut Parser) {
    let m = p.start();
    p.bump();
    p.expect("(");
    while !p.at_end() && !p.at(")") {
        let d = p.start();
        p.eat("parameter");
        p.eat("localparam");
        if p.eat("type") {
            if p.at_any(&["enum", "struct", "union", "class", "interface"]) {
                // 1800-2023 type restriction: `type class T`.
                p.bump();
                p.eat("class");
            }
            name_token(p);
            if p.eat("=") {
                data_type(p);
            }
        } else {
            if !(p.at_ident() && matches!(p.nth(1), "=" | "," | ")" | "[")) {
                data_type(p);
            }
            let dl = p.start();
            name_token(p);
            while p.at("[") {
                dim(p);
            }
            if p.eat("=") {
                crate::expr::type_or_expr(p);
            }
            dl.complete(p, Declarator);
        }
        d.complete(p, ParamDecl);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(")");
    m.complete(p, ParamList);
}

const DIRECTIONS: &[&str] = &["input", "output", "inout", "ref"];

/// `( port, ... )` of a module or a function/task.
pub(crate) fn port_list(p: &mut Parser) {
    let m = p.start();
    p.bump();
    while !p.at_end() && !p.at(")") && !p.at(";") {
        let before = p.pos();
        port(p);
        if !p.eat(",") {
            if p.pos() == before {
                p.error(format!("unexpected `{}` in port list", p.cur()));
                p.bump();
                continue;
            }
            break;
        }
    }
    p.expect(")");
    m.complete(p, PortList);
}

fn port(p: &mut Parser) {
    let m = p.start();
    attributes(p);
    if p.at(".") {
        // `.name(expr)`
        p.bump();
        name_token(p);
        p.expect("(");
        if !p.at(")") {
            expr(p);
        }
        p.expect(")");
        m.complete(p, Port);
        return;
    }
    p.eat("local");
    p.eat("const");
    if p.at_any(DIRECTIONS) {
        p.bump();
    }
    p.eat("var");
    p.eat("static");
    p.eat("automatic");
    if is_net_type(p.cur()) {
        p.bump();
    }
    if p.at("interface") {
        p.bump();
        if p.eat(".") {
            name_token(p);
        }
    } else if p.at_ident() && p.nth(1) == "." && p.nth_kind(3).is_some() {
        // `bus_if.mp b`
        p.bump();
        p.bump();
        p.bump();
    } else if p.at_ident() && matches!(p.nth(1), "," | ")" | "[" | "=") {
        // Name only; the type is implicit or inherited.
    } else if !p.at_any(&[",", ")"]) {
        data_type(p);
    }
    if p.at_ident() {
        let d = p.start();
        name_token(p);
        while p.at("[") {
            dim(p);
        }
        if p.eat("=") {
            expr(p);
        }
        d.complete(p, Declarator);
    }
    m.complete(p, Port);
}

// ---- timing -----------------------------------------------------------

/// `#5`, `#(d)`, `#1ns`, `##[1:3]`, `@(posedge clk iff en)`, `@e`, `@*`,
/// `repeat (n) @e`.
pub(crate) fn timing_control(p: &mut Parser) {
    let m = p.start();
    let op = p.cur();
    p.bump();
    match op {
        "repeat" => {
            paren_expr(p);
            if p.at_any(&["@", "#"]) {
                timing_control(p);
            }
        }
        "#" | "##" => {
            if p.at("(") {
                // `#(d)`, `#(min:typ:max)`, gate delays `#(rise, fall, off)`.
                p.bump();
                loop {
                    expr(p);
                    if p.eat(":") {
                        expr(p);
                        p.expect(":");
                        expr(p);
                    }
                    if !p.eat(",") {
                        break;
                    }
                }
                p.expect(")");
            } else if p.at("[") {
                p.bump();
                if p.at_any(&["*", "+"]) && p.nth(1) == "]" {
                    p.bump();
                } else {
                    expr(p);
                    if p.eat(":") {
                        if !p.eat("$") {
                            expr(p);
                        }
                    }
                }
                p.expect("]");
            } else {
                delay_value(p);
            }
        }
        _ => {
            // `@` / `@@`
            if p.at("*") {
                p.bump();
            } else if p.at("(") && p.nth(1) == "*" && p.nth(2) == ")" {
                p.bump();
                p.bump();
                p.bump();
            } else if p.at("(") {
                p.bump();
                event_expr(p);
                p.expect(")");
            } else {
                hier_name(p);
            }
        }
    }
    m.complete(p, TimingControl);
}

/// A delay without parentheses: a literal (with `1step`) or a name.
fn delay_value(p: &mut Parser) {
    let m = p.start();
    if p.at_ident() || p.at_any(&["$unit", "local"]) {
        hier_name(p);
    } else {
        p.bump();
        if p.at("step") || (p.at_ident() && p.cur() == "step") {
            p.bump();
        }
    }
    m.complete(p, Literal);
}

/// `a.b.c`, `pkg::x`, with selects: an event or delay name.
fn hier_name(p: &mut Parser) {
    let m = p.start();
    p.bump();
    loop {
        if p.at_any(&[".", "::"]) {
            p.bump();
            p.bump();
        } else if p.at("[") {
            p.bump();
            expr(p);
            p.expect("]");
        } else {
            break;
        }
    }
    m.complete(p, NameExpr);
}

/// `[edge] expr [iff expr] {(or | ,) ...}`, with parenthesized groups.
pub(crate) fn event_expr(p: &mut Parser) {
    loop {
        let m = p.start();
        if p.at("(") && is_paren_event(p) {
            p.bump();
            event_expr(p);
            p.expect(")");
        } else {
            if p.at_any(&["posedge", "negedge", "edge"]) {
                p.bump();
            }
            expr(p);
            if p.eat("iff") {
                expr(p);
            }
        }
        m.complete(p, EventExpr);
        if !(p.eat("or") || p.eat(",")) {
            break;
        }
    }
}

/// `(` that groups events rather than starting an expression: it holds an
/// edge keyword, `or` or `iff` at its own level.
fn is_paren_event(p: &Parser) -> bool {
    let mut i = p.pos() + 1;
    let mut depth = 0i32;
    loop {
        match p.text_at(i) {
            "" => return false,
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            "posedge" | "negedge" | "edge" | "or" | "iff" if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
}

// ---- constraints ------------------------------------------------------

const NO_IMPLICATION: Flags = Flags { assign: false, nonblocking: false, no_implication: true };

/// `{ constraint_item ... }`
pub(crate) fn constraint_block(p: &mut Parser) {
    let b = p.start();
    p.expect("{");
    while !p.at_end() && !p.at("}") && !p.at_any(END_KEYWORDS) {
        let before = p.pos();
        constraint_item(p);
        if p.pos() == before {
            p.error(format!("unexpected `{}` in constraint", p.cur()));
            p.bump();
        }
    }
    p.expect("}");
    b.complete(p, ConstraintBlock);
}

fn constraint_set(p: &mut Parser) {
    if p.at("{") {
        constraint_block(p);
    } else {
        constraint_item(p);
    }
}

fn constraint_item(p: &mut Parser) {
    let m = p.start();
    match p.cur() {
        "{" => {
            constraint_block(p);
        }
        "soft" => {
            p.bump();
            expr_or_dist(p);
            p.expect(";");
        }
        "solve" | "disable" => {
            to_semi(p);
        }
        "if" => {
            p.bump();
            paren_expr(p);
            constraint_set(p);
            if p.eat("else") {
                constraint_set(p);
            }
        }
        "foreach" => {
            p.bump();
            group(p);
            constraint_set(p);
        }
        "unique" => {
            p.bump();
            range_list(p);
            p.expect(";");
        }
        _ => {
            expr_or_dist(p);
            if p.eat("->") {
                constraint_set(p);
            } else {
                p.expect(";");
            }
        }
    }
    m.complete(p, ConstraintItem);
}

fn expr_or_dist(p: &mut Parser) {
    let m = p.start();
    expr_with(p, NO_IMPLICATION);
    if p.at("dist") {
        p.bump();
        let d = p.start();
        p.expect("{");
        while !p.at_end() && !p.at("}") {
            let i = p.start();
            if !p.eat("default") {
                value_range(p);
            }
            if p.at_any(&[":=", ":/"]) {
                p.bump();
                expr(p);
            }
            i.complete(p, DistItem);
            if !p.eat(",") {
                break;
            }
        }
        p.expect("}");
        d.complete(p, RangeList);
        m.complete(p, ExprOrDist);
    } else {
        m.abandon(p);
    }
}

// ---- covergroups ------------------------------------------------------

/// `covergroup name [(ports)] [event | with function sample(..)] ; ... endgroup`
pub(crate) fn covergroup(p: &mut Parser) {
    p.bump();
    if p.eat("extends") {
        // 1800-2023 `covergroup extends base;`
    }
    name_token(p);
    if p.at("(") {
        port_list(p);
    }
    if p.at_any(&["@", "@@"]) {
        if p.at("@@") {
            // Block event: `@@(begin f)`.
            let t = p.start();
            p.bump();
            group(p);
            t.complete(p, BlockEvent);
        } else {
            timing_control(p);
        }
    } else if p.at("with") {
        let w = p.start();
        p.bump();
        p.expect("function");
        p.bump(); // sample
        if p.at("(") {
            port_list(p);
        }
        w.complete(p, WithClause);
    }
    p.expect(";");
    while !p.at_end() && !p.at("endgroup") && !p.at_any(&END_KEYWORDS[..21]) {
        let before = p.pos();
        coverage_item(p);
        if p.pos() == before {
            p.error(format!("unexpected `{}` in covergroup", p.cur()));
            p.bump();
        }
    }
    p.expect("endgroup");
    end_label(p);
}

fn coverage_item(p: &mut Parser) {
    let m = p.start();
    attributes(p);
    if matches!(p.cur(), "option" | "type_option") {
        coverage_option(p);
        m.complete(p, CoverageOption);
        return;
    }
    if p.at(";") {
        p.bump();
        m.complete(p, EmptyItem);
        return;
    }
    if p.at("function") {
        subroutine(p, 0, false);
        m.complete(p, FunctionDecl);
        return;
    }
    // `[type] [label :] coverpoint expr` / `[label :] cross a, b`
    if p.at_ident() && p.nth(1) == ":" {
        let l = p.start();
        p.bump();
        p.bump();
        l.complete(p, StmtLabel);
    } else if !p.at_any(&["coverpoint", "cross"]) {
        // Data type in front of a coverpoint: `bit [3:0] cp: coverpoint x`.
        let mut i = p.pos();
        while !matches!(p.text_at(i), "coverpoint" | ";" | "") {
            i += 1;
        }
        if p.text_at(i) == "coverpoint" {
            data_type(p);
            if p.at_ident() && p.nth(1) == ":" {
                let l = p.start();
                p.bump();
                p.bump();
                l.complete(p, StmtLabel);
            }
        }
    }
    let kind = if p.eat("coverpoint") {
        expr(p);
        Coverpoint
    } else if p.eat("cross") {
        loop {
            expr_bp(p, 30, Flags::default());
            if !p.eat(",") {
                break;
            }
        }
        Cross
    } else {
        p.error(format!("expected `coverpoint` or `cross`, found `{}`", p.cur()));
        to_semi(p);
        m.complete(p, OtherItem);
        return;
    };
    if p.eat("iff") {
        paren_expr(p);
    }
    if p.at("{") {
        p.bump();
        while !p.at_end() && !p.at("}") && !p.at("endgroup") {
            let before = p.pos();
            bins_item(p);
            if p.pos() == before {
                p.error(format!("unexpected `{}` in bins", p.cur()));
                p.bump();
            }
        }
        p.expect("}");
    } else {
        p.expect(";");
    }
    m.complete(p, kind);
}

/// `option.name = value;` (Slang keeps the whole assignment).
fn coverage_option(p: &mut Parser) {
    expr_with(p, crate::expr::ASSIGNABLE);
    p.expect(";");
}

fn bins_item(p: &mut Parser) {
    let m = p.start();
    attributes(p);
    if matches!(p.cur(), "option" | "type_option") {
        coverage_option(p);
        m.complete(p, CoverageOption);
        return;
    }
    if p.at(";") {
        p.bump();
        m.complete(p, EmptyItem);
        return;
    }
    if p.at("function") {
        subroutine(p, 0, false);
        m.complete(p, FunctionDecl);
        return;
    }
    p.eat("wildcard");
    if !p.at_any(&["bins", "illegal_bins", "ignore_bins"]) {
        p.error(format!("expected bins, found `{}`", p.cur()));
        to_semi(p);
        m.complete(p, OtherItem);
        return;
    }
    p.bump();
    name_token(p);
    if p.at("[") {
        p.bump();
        if !p.at("]") {
            expr(p);
        }
        p.expect("]");
    }
    p.expect("=");
    if p.at("{") {
        range_list(p);
        if p.at("with") {
            with_expr(p);
        }
    } else if p.at("default") {
        p.bump();
        p.eat("sequence");
    } else if p.at("(") && is_transition(p) {
        loop {
            trans_list(p);
            if !p.eat(",") {
                break;
            }
        }
    } else {
        bins_select(p, 0);
        if p.at("with") {
            with_expr(p);
        }
    }
    if p.eat("iff") {
        paren_expr(p);
    }
    p.expect(";");
    m.complete(p, Bins);
}

fn with_expr(p: &mut Parser) {
    let w = p.start();
    p.bump();
    paren_expr(p);
    w.complete(p, WithClause);
}

/// `( a => b [*2] , c )`: a parenthesized list with `=>` at its level.
fn is_transition(p: &Parser) -> bool {
    let mut i = p.pos() + 1;
    let mut depth = 0i32;
    loop {
        match p.text_at(i) {
            "" | ";" => return false,
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            "=>" if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
}

fn trans_list(p: &mut Parser) {
    let m = p.start();
    p.expect("(");
    loop {
        let s = p.start();
        loop {
            value_range(p);
            if !p.eat(",") {
                break;
            }
        }
        if p.at("[") && matches!(p.nth(1), "*" | "->" | "=") {
            p.bump();
            p.bump();
            expr(p);
            if p.eat(":") {
                expr(p);
            }
            p.expect("]");
        }
        s.complete(p, TransItem);
        if !p.eat("=>") {
            break;
        }
    }
    p.expect(")");
    m.complete(p, TransList);
}

/// Cross bins selection: `binsof(a) intersect {..} && !binsof(b.x)`.
/// `&&` and `||` here combine selections; they are not expressions.
fn bins_select(p: &mut Parser, min_bp: u8) {
    let m = p.start();
    bins_select_primary(p);
    let mut done = m.complete(p, BinsSelect);
    loop {
        let bp = match p.cur() {
            "||" => 1,
            "&&" => 2,
            _ => break,
        };
        if bp < min_bp {
            break;
        }
        let m = done.precede(p);
        p.bump();
        bins_select(p, bp + 1);
        done = m.complete(p, BinsSelect);
    }
}

fn bins_select_primary(p: &mut Parser) {
    if p.eat("!") {
        bins_select_primary(p);
    } else if p.at("binsof") {
        p.bump();
        p.expect("(");
        expr_bp(p, 30, Flags::default());
        p.expect(")");
        if p.eat("intersect") {
            range_list(p);
        }
    } else if p.at("(") && is_paren_select(p) {
        p.bump();
        bins_select(p, 0);
        p.expect(")");
    } else {
        expr_bp(p, 12, Flags::default());
        if p.eat("matches") {
            expr(p);
        }
    }
}

fn is_paren_select(p: &Parser) -> bool {
    let mut i = p.pos() + 1;
    let mut depth = 0i32;
    loop {
        match p.text_at(i) {
            "" | ";" => return false,
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            "binsof" => return true,
            _ => {}
        }
        i += 1;
    }
}
