//! Grammar: design units, classes, packages and their items, and
//! statements (slice 1 of the parser). The constructs inside them are
//! parsed in place by the later slices: expressions in `expr`, headers,
//! constraints and covergroups in `blocks`, assertions in `assertions`
//! and the rarer items in `misc`.

pub(crate) use crate::blocks::{constraint_block, timing_control};
use crate::expr::{arg_list, expr, expr_bp, expr_with, param_values, value_range, ASSIGNABLE, STATEMENT};
use crate::parser::Parser;
use crate::SyntaxKind::{self, *};

/// Keywords that close a scope. A statement or item never consumes one it
/// does not own, which keeps one bad construct from swallowing the rest.
pub(crate) const END_KEYWORDS: &[&str] = &[
    "end",
    "join",
    "join_any",
    "join_none",
    "endcase",
    "endfunction",
    "endtask",
    "endclass",
    "endmodule",
    "endinterface",
    "endprogram",
    "endpackage",
    "endchecker",
    "endgenerate",
    "endgroup",
    "endproperty",
    "endsequence",
    "endclocking",
    "endspecify",
    "endconfig",
    "endprimitive",
    "endtable",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ctx {
    Top,
    Module,
    Package,
    Class,
    Generate,
}

pub(crate) fn source_file(p: &mut Parser) {
    let m = p.start();
    while !p.at_end() {
        if !item(p, Ctx::Top) {
            stray(p);
        }
    }
    m.complete(p, SourceFile);
}

/// A token nothing else claims, kept in an Error node.
fn stray(p: &mut Parser) {
    let m = p.start();
    p.error(format!("unexpected `{}`", p.cur()));
    p.bump();
    m.complete(p, Error);
}

pub(crate) fn attributes(p: &mut Parser) {
    crate::misc::attributes(p);
}

/// Consumes one balanced group starting at an opening bracket.
pub(crate) fn group(p: &mut Parser) {
    let close = match p.cur() {
        "(" => ")",
        "[" => "]",
        "{" | "'{" => "}",
        _ => {
            p.bump();
            return;
        }
    };
    p.bump();
    while !p.at_end() && !p.at(close) {
        if p.at_any(&["(", "[", "{", "'{"]) {
            group(p);
        } else if p.at_any(&[")", "]", "}"]) || p.at_any(END_KEYWORDS) {
            // Mismatched closer: leave it to the caller.
            p.error(format!("expected `{close}`"));
            return;
        } else {
            p.bump();
        }
    }
    p.expect(close);
}

/// Consumes tokens up to and including the `;` that ends this construct,
/// stepping over bracketed groups. Stops before an end keyword.
pub(crate) fn to_semi(p: &mut Parser) {
    while !p.at_end() {
        if p.at(";") {
            p.bump();
            return;
        }
        if p.at_any(END_KEYWORDS) {
            p.error("expected `;`".into());
            return;
        }
        if p.at_any(&["(", "[", "{", "'{"]) {
            group(p);
        } else {
            p.bump();
        }
    }
    p.error("expected `;`".into());
}

/// Consumes tokens until `end_kw` (inclusive) with an optional end label.
pub(crate) fn end_label(p: &mut Parser) {
    if p.at(":") && p.nth(1) != ":" {
        let m = p.start();
        p.bump();
        p.bump();
        m.complete(p, EndLabel);
    }
}

pub(crate) fn name_token(p: &mut Parser) {
    let m = p.start();
    if p.at_ident() || p.nth_kind(0) == Some(sv_lexer::Kind::Keyword) {
        if !p.at_ident() {
            // `new` and a few other keywords name methods.
            if p.cur() != "new" {
                p.error(format!("expected a name, found `{}`", p.cur()));
            }
        }
        p.bump();
    } else {
        p.error(format!("expected a name, found `{}`", p.cur()));
    }
    m.complete(p, Name);
}

const QUALIFIERS: &[&str] = &[
    "extern",
    "pure",
    "virtual",
    "static",
    "protected",
    "local",
    "rand",
    "randc",
    "const",
    "automatic",
    "var",
    "forkjoin",
];

/// Parses one item of a design unit, package, class or generate region.
/// Returns false when nothing could start here.
pub(crate) fn item(p: &mut Parser, ctx: Ctx) -> bool {
    let m = p.start();
    attributes(p);
    let kind = item_body(p, ctx);
    match kind {
        Some(k) => {
            m.complete(p, k);
            true
        }
        None => {
            m.abandon(p);
            false
        }
    }
}

fn item_body(p: &mut Parser, ctx: Ctx) -> Option<SyntaxKind> {
    // Qualifiers decide between a method, a constraint and a property.
    let mut q = 0;
    while QUALIFIERS.contains(&p.nth(q)) {
        q += 1;
    }
    let head = p.nth(q);
    if q > 0 {
        if head == "function" || head == "task" {
            let proto = (0..q).any(|i| p.nth(i) == "extern")
                || ((0..q).any(|i| p.nth(i) == "pure") && (0..q).any(|i| p.nth(i) == "virtual"));
            return Some(subroutine(p, q, proto));
        }
        if head == "constraint" {
            return Some(constraint(p, q));
        }
        if head == "class" {
            return Some(class_decl(p, q));
        }
        if p.at("extern") && matches!(head, "module" | "macromodule" | "interface" | "program" | "primitive") {
            to_semi(p);
            return Some(OtherItem);
        }
        if ctx == Ctx::Top && p.at("virtual") && head == "interface" && p.nth(q + 1) == "class" {
            return Some(class_decl(p, q + 1));
        }
        data_decl(p);
        return Some(DataDecl);
    }
    let k = match p.cur() {
        "" => return None,
        ";" => {
            p.bump();
            EmptyItem
        }
        "module" | "macromodule" => unit(p, "endmodule", ModuleDecl),
        "interface" if p.nth(1) == "class" => class_decl(p, 1),
        "interface" => unit(p, "endinterface", InterfaceDecl),
        "program" => unit(p, "endprogram", ProgramDecl),
        "checker" => unit(p, "endchecker", CheckerDecl),
        "package" => unit(p, "endpackage", PackageDecl),
        "class" => class_decl(p, 0),
        "function" | "task" => subroutine(p, 0, false),
        "typedef" => typedef(p),
        "constraint" => constraint(p, 0),
        "import" if p.nth_kind(1) == Some(sv_lexer::Kind::StringLiteral) => {
            crate::assertions::dpi(p);
            DpiImport
        }
        "export" if p.nth_kind(1) == Some(sv_lexer::Kind::StringLiteral) => {
            crate::assertions::dpi(p);
            DpiExport
        }
        "import" => {
            to_semi(p);
            ImportDecl
        }
        "export" => {
            to_semi(p);
            ExportDecl
        }
        "parameter" | "localparam" => {
            param_decl(p);
            ParamDecl
        }
        "defparam" => {
            p.bump();
            loop {
                expr_bp(p, 30, Default::default());
                p.expect("=");
                expr(p);
                if !p.eat(",") {
                    break;
                }
            }
            p.expect(";");
            ParamDecl
        }
        "specparam" => crate::misc::specparam(p),
        "nettype" => crate::misc::nettype(p),
        "alias" => crate::misc::net_alias(p),
        // `$static_assert` is a Slang extension, accepted as an item.
        "$fatal" | "$error" | "$warning" | "$info" | "$static_assert" => {
            expr(p);
            p.expect(";");
            ElabTask
        }
        "timeunit" | "timeprecision" => {
            to_semi(p);
            TimeunitDecl
        }
        "genvar" => {
            to_semi(p);
            GenvarDecl
        }
        "modport" => {
            crate::assertions::modport(p);
            ModportDecl
        }
        "bind" => {
            crate::assertions::bind(p);
            BindDirective
        }
        "let" => {
            crate::assertions::let_decl(p);
            LetDecl
        }
        "assign" => {
            p.bump();
            strength(p);
            if p.at("#") {
                timing_control(p);
            }
            loop {
                expr_with(p, ASSIGNABLE);
                if !p.eat(",") {
                    break;
                }
            }
            p.expect(";");
            ContinuousAssign
        }
        "covergroup" => {
            crate::blocks::covergroup(p);
            CovergroupDecl
        }
        "property" => {
            crate::assertions::property_decl(p, "endproperty");
            PropertyDecl
        }
        "sequence" => {
            crate::assertions::property_decl(p, "endsequence");
            SequenceDecl
        }
        "clocking" => {
            crate::assertions::clocking(p);
            ClockingDecl
        }
        "default" | "global" if p.nth(1) == "clocking" => {
            crate::assertions::clocking(p);
            ClockingDecl
        }
        "default" if p.nth(1) == "disable" => {
            crate::assertions::default_disable(p);
            DefaultDisable
        }
        "config" => crate::misc::config(p),
        "primitive" => crate::misc::udp(p),
        "specify" => crate::misc::specify(p),
        "generate" => {
            p.bump();
            while !p.at_end() && !p.at("endgenerate") && !at_foreign_end(p, "endgenerate") {
                if !item(p, Ctx::Generate) {
                    stray(p);
                }
            }
            p.expect("endgenerate");
            GenerateRegion
        }
        "always" | "always_comb" | "always_ff" | "always_latch" | "initial" | "final" => {
            p.bump();
            statement(p);
            ProceduralBlock
        }
        "assert" | "assume" | "cover" | "restrict" | "expect" => assertion_item(p),
        "for" if ctx != Ctx::Class => {
            p.bump();
            p.expect("(");
            p.eat("genvar");
            name_token(p);
            p.expect("=");
            expr(p);
            p.expect(";");
            expr(p);
            p.expect(";");
            expr_with(p, ASSIGNABLE);
            p.expect(")");
            generate_block(p);
            LoopGenerate
        }
        "if" if ctx != Ctx::Class => {
            p.bump();
            paren_expr(p);
            generate_block(p);
            if p.at("else") {
                let e = p.start();
                p.bump();
                generate_block(p);
                e.complete(p, ElseClause);
            }
            IfGenerate
        }
        "case" if ctx != Ctx::Class => {
            p.bump();
            paren_expr(p);
            while !p.at_end() && !p.at("endcase") && !p.at_any(&END_KEYWORDS[5..]) {
                let before = p.pos();
                if p.at("default") {
                    p.bump();
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
                generate_block(p);
                if p.pos() == before {
                    stray(p);
                }
            }
            p.expect("endcase");
            CaseGenerate
        }
        "begin" if ctx != Ctx::Class => {
            generate_block_inner(p);
            GenerateBlock
        }
        s if END_KEYWORDS.contains(&s) => return None,
        _ => {
            // `label: assert property (...)`
            if p.at_ident() && p.nth(1) == ":" && matches!(p.nth(2), "assert" | "assume" | "cover" | "restrict") {
                let l = p.start();
                p.bump();
                p.bump();
                l.complete(p, StmtLabel);
                return Some(assertion_item(p));
            }
            if GATES.contains(&p.cur()) {
                gate_instantiation(p);
                return Some(GateInst);
            }
            if ctx != Ctx::Class && is_instantiation(p) {
                instantiation(p);
                return Some(Instantiation);
            }
            if is_decl_start(p) {
                data_decl(p);
                return Some(DataDecl);
            }
            // Nothing else starts an item here.
            p.error(format!("unexpected `{}`; expected an item", p.cur()));
            to_semi(p);
            OtherItem
        }
    };
    Some(k)
}

/// An assertion among module items.
fn assertion_item(p: &mut Parser) -> SyntaxKind {
    if p.at("expect") || matches!(p.nth(1), "property" | "sequence") {
        crate::assertions::concurrent_assertion_in(p, true);
        return ConcurrentAssert;
    }
    statement_body(p)
}

/// True at an end keyword of an enclosing construct other than `own`.
fn at_foreign_end(p: &Parser, own: &str) -> bool {
    p.at_any(END_KEYWORDS) && !p.at(own) && !matches!(p.cur(), "end" | "endcase")
}

fn generate_block(p: &mut Parser) {
    if p.at("begin") || (p.at_ident() && p.nth(1) == ":" && p.nth(2) == "begin") {
        let m = p.start();
        // `label : begin ... end`
        if !p.at("begin") {
            let l = p.start();
            p.bump();
            p.bump();
            l.complete(p, StmtLabel);
        }
        generate_block_inner(p);
        m.complete(p, GenerateBlock);
    } else if !item(p, Ctx::Generate) {
        stray(p);
    }
}

fn generate_block_inner(p: &mut Parser) {
    p.bump();
    end_label(p);
    while !p.at_end() && !p.at("end") && !at_foreign_end(p, "end") {
        if !item(p, Ctx::Generate) {
            stray(p);
        }
    }
    p.expect("end");
    end_label(p);
}

/// module / interface / program / checker / package.
fn unit(p: &mut Parser, end_kw: &str, kind: SyntaxKind) -> SyntaxKind {
    p.bump();
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    name_token(p);
    crate::blocks::module_header(p);
    let ctx = if kind == PackageDecl { Ctx::Package } else { Ctx::Module };
    while !p.at_end() && !p.at(end_kw) && !at_foreign_end(p, end_kw) {
        if !item(p, ctx) {
            stray(p);
        }
    }
    p.expect(end_kw);
    end_label(p);
    kind
}

/// `[virtual|interface] class [lifetime] name ... ; items endclass`.
/// `q` qualifier tokens come before `class`.
fn class_decl(p: &mut Parser, q: usize) -> SyntaxKind {
    for _ in 0..q {
        p.bump();
    }
    p.expect("class");
    override_specifiers(p);
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    name_token(p);
    crate::blocks::class_header(p);
    while !p.at_end() && !p.at("endclass") && !at_foreign_end(p, "endclass") {
        if !item(p, Ctx::Class) {
            stray(p);
        }
    }
    p.expect("endclass");
    end_label(p);
    ClassDecl
}

/// 1800-2023 `:initial`, `:extends`, `:final` after `class`, `function`,
/// `task` or `constraint`.
pub(crate) fn override_specifiers(p: &mut Parser) {
    while p.at(":") && matches!(p.nth(1), "initial" | "extends" | "final") {
        p.bump();
        p.bump();
    }
}

/// Finds the method name in a header `[lifetime] [type] name (` or `;`:
/// the trailing `id (:: id)*` chain, where a scope may carry `#( ... )`.
/// Returns (name start, name end) as absolute token indices.
fn find_method_name(p: &Parser) -> Option<(usize, usize)> {
    let mut i = p.pos();
    let limit = i + 400;
    loop {
        match p.text_at(i) {
            // `,` and `)` end a modport's `task t` with no ports.
            "(" | ";" | "" | "," | ")" => break,
            "#" if p.text_at(i + 1) == "(" => i = skip_group_at(p, i + 1),
            "[" | "{" => i = skip_group_at(p, i),
            _ => i += 1,
        }
        if i > limit {
            return None;
        }
    }
    let end = i;
    if end == p.pos() {
        return None;
    }
    let mut s = end - 1;
    // Walk back over `scope ::` pairs (and an interface port's `i.`).
    while s >= p.pos() + 2 && matches!(p.text_at(s - 1), "::" | ".") {
        let mut t = s - 2;
        if p.text_at(t) == ")" {
            // `C#(T)::f`: back to the matching `(` and its `#`.
            let mut depth = 0;
            loop {
                match p.text_at(t) {
                    ")" => depth += 1,
                    "(" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                if t == p.pos() {
                    return Some((s, end));
                }
                t -= 1;
            }
            if t < p.pos() + 2 || p.text_at(t - 1) != "#" {
                break;
            }
            t -= 2;
        }
        s = t;
    }
    Some((s, end))
}

/// Absolute index just past the balanced group opening at `i`.
fn skip_group_at(p: &Parser, mut i: usize) -> usize {
    let mut depth = 0i32;
    loop {
        match p.text_at(i) {
            "(" | "[" | "{" | "'{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            "" => return i,
            _ => {}
        }
        i += 1;
    }
}

/// function / task, declaration or prototype. `q` qualifier tokens first.
pub(crate) fn subroutine(p: &mut Parser, q: usize, proto: bool) -> SyntaxKind {
    for _ in 0..q {
        p.bump();
    }
    let is_task = p.at("task");
    p.bump();
    override_specifiers(p);
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    override_specifiers(p);
    match find_method_name(p) {
        Some((s, e)) => {
            if s > p.pos() {
                let r = p.start();
                data_type(p);
                while p.pos() < s {
                    p.bump();
                }
                r.complete(p, ReturnType);
            }
            let n = p.start();
            while p.pos() < e {
                p.bump();
            }
            n.complete(p, Name);
        }
        None => p.error("expected a function or task name".into()),
    }
    if p.at("(") {
        crate::blocks::port_list(p);
    }
    if proto && p.at_any(&[",", ")"]) {
        // A modport's `import task t(int a), ...`.
        return MethodPrototype;
    }
    p.expect(";");
    if proto {
        return MethodPrototype;
    }
    let end_kw = if is_task { "endtask" } else { "endfunction" };
    let b = p.start();
    while !p.at_end() && !p.at(end_kw) && !at_foreign_end(p, end_kw) {
        if !statement(p) {
            stray(p);
        }
    }
    b.complete(p, Body);
    p.expect(end_kw);
    end_label(p);
    if is_task {
        TaskDecl
    } else {
        FunctionDecl
    }
}

fn typedef(p: &mut Parser) -> SyntaxKind {
    // Forward: `typedef [class|enum|struct|union|interface class] name;`
    let mut i = 1;
    if p.nth(i) == "interface" && p.nth(i + 1) == "class" {
        i += 2;
    } else if matches!(p.nth(i), "class" | "enum" | "struct" | "union") {
        i += 1;
    }
    let ident = matches!(p.nth_kind(i), Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier));
    if ident && p.nth(i + 1) == ";" {
        for _ in 0..i {
            p.bump();
        }
        name_token(p);
        p.bump();
        return ForwardTypedef;
    }
    // Full typedef: the name is the last identifier before `;`, ahead of
    // any unpacked dimensions.
    let start = p.pos();
    let mut j = start + 1;
    let mut name_at = None;
    loop {
        match p.text_at(j) {
            ";" | "" => break,
            "(" | "[" | "{" | "'{" => j = skip_group_at(p, j),
            _ => {
                if matches!(p.kind_at(j), Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier)) {
                    name_at = Some(j);
                }
                j += 1;
            }
        }
    }
    let Some(n) = name_at else {
        to_semi(p);
        p.error("typedef without a name".into());
        return TypedefDecl;
    };
    p.bump();
    data_type(p);
    while p.pos() < n {
        // What data_type left over (unusual forms) stays as written.
        if p.at_any(&["(", "[", "{", "'{"]) {
            group(p);
        } else {
            p.bump();
        }
    }
    name_token(p);
    while p.at("[") {
        dim(p);
    }
    p.expect(";");
    TypedefDecl
}

/// `[static] constraint name { ... }` or a prototype `constraint name;`.
fn constraint(p: &mut Parser, q: usize) -> SyntaxKind {
    for _ in 0..q {
        p.bump();
    }
    p.bump();
    override_specifiers(p);
    let n = p.start();
    while !p.at_end() && !p.at_any(&["{", ";"]) {
        p.bump();
    }
    n.complete(p, Name);
    if p.at("{") {
        constraint_block(p);
        ConstraintDecl
    } else {
        p.expect(";");
        ConstraintPrototype
    }
}

// ---- types and declarations -------------------------------------------

/// Keywords that start a data type.
pub(crate) const BUILTIN_TYPES: &[&str] = &[
    "bit",
    "logic",
    "reg",
    "byte",
    "shortint",
    "int",
    "longint",
    "integer",
    "time",
    "real",
    "shortreal",
    "realtime",
    "string",
    "chandle",
    "event",
    "void",
    "enum",
    "struct",
    "union",
];

pub(crate) fn is_net_type(s: &str) -> bool {
    NET_TYPES.contains(&s)
}

const NET_TYPES: &[&str] = &[
    "wire",
    "tri",
    "tri0",
    "tri1",
    "triand",
    "trior",
    "trireg",
    "wand",
    "wor",
    "supply0",
    "supply1",
    "uwire",
    "interconnect",
];

const GATES: &[&str] = &[
    "and", "nand", "or", "nor", "xor", "xnor", "buf", "not", "bufif0", "bufif1", "notif0", "notif1", "nmos", "pmos",
    "rnmos", "rpmos", "cmos", "rcmos", "tran", "rtran", "tranif0", "tranif1", "rtranif0", "rtranif1", "pullup",
    "pulldown",
];

const STRENGTHS: &[&str] = &[
    "supply0", "strong0", "pull0", "weak0", "highz0", "supply1", "strong1", "pull1", "weak1", "highz1", "small",
    "medium", "large",
];

const DECL_QUALIFIERS: &[&str] = &[
    "const",
    "var",
    "static",
    "automatic",
    "rand",
    "randc",
    "local",
    "protected",
    "virtual",
    "input",
    "output",
    "inout",
    "ref",
];

fn ident_at(p: &Parser, i: usize) -> bool {
    matches!(p.kind_at(i), Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier))
}

/// Index past a type name `a [#(..)] {:: b [#(..)]}` starting at `i`, or
/// None if no name is there.
fn type_name_end(p: &Parser, mut i: usize) -> Option<usize> {
    if !(ident_at(p, i) || (matches!(p.text_at(i), "$unit" | "local" | "std") && p.text_at(i + 1) == "::")) {
        return None;
    }
    i += 1;
    loop {
        if p.text_at(i) == "#" && p.text_at(i + 1) == "(" {
            i = skip_group_at(p, i + 1);
        }
        if p.text_at(i) == "::" && (ident_at(p, i + 1) || p.kind_at(i + 1) == Some(sv_lexer::Kind::Keyword)) {
            i += 2;
            continue;
        }
        return Some(i);
    }
}

/// Lookahead: does a data declaration start here (in a block or item)?
pub(crate) fn is_decl_start(p: &Parser) -> bool {
    let mut i = p.pos();
    while DECL_QUALIFIERS.contains(&p.text_at(i)) {
        i += 1;
    }
    let t = p.text_at(i);
    if i > p.pos() && matches!(p.text_at(p.pos()), "input" | "output" | "inout" | "ref") {
        // Non-ANSI port declaration: `input a;`, `output [3:0] b, c;`.
        return true;
    }
    if p.text_at(i + 1) == "'" {
        // A cast: `void'(f())`, `int'(x)`.
        return false;
    }
    if BUILTIN_TYPES.contains(&t) || NET_TYPES.contains(&t) || matches!(t, "signed" | "unsigned") {
        return true;
    }
    if t == "virtual" || t == "interface" {
        return true;
    }
    if t == "type" && p.text_at(i + 1) == "(" {
        return true;
    }
    let Some(mut j) = type_name_end(p, i) else { return false };
    if i > p.pos() && matches!(p.text_at(j), "=" | ";" | ",") {
        // `var x = 1;`, `const x = 1;`: no type.
        return true;
    }
    if p.text_at(j) == "." && ident_at(p, j + 1) && ident_at(p, j + 2) {
        // `I.mp x;`
        return true;
    }
    if p.text_at(j) == "#" && p.text_at(j + 1) != "(" && ident_at(p, j + 2) {
        // A user nettype with a delay: `nt #3 b;`.
        return true;
    }
    while p.text_at(j) == "[" {
        j = skip_group_at(p, j);
    }
    ident_at(p, j)
}

/// Lookahead: `type [#(..)] name [dims] (` is a module instantiation.
fn is_instantiation(p: &Parser) -> bool {
    let i = p.pos();
    if !ident_at(p, i) {
        return false;
    }
    let mut j = i + 1;
    // `pkg::checker_name inst (...)`
    while p.text_at(j) == "::" && ident_at(p, j + 1) {
        j += 2;
    }
    if p.text_at(j) == "#" {
        if p.text_at(j + 1) == "(" {
            j = skip_group_at(p, j + 1);
        } else {
            j += 2;
        }
    }
    if !ident_at(p, j) {
        return false;
    }
    j += 1;
    while p.text_at(j) == "[" {
        j = skip_group_at(p, j);
    }
    p.text_at(j) == "("
}

/// A data type, or an implicit one (`signed [3:0]`, `[7:0]`).
pub(crate) fn data_type(p: &mut Parser) {
    let m = p.start();
    match p.cur() {
        "enum" => {
            p.bump();
            if !p.at("{") {
                data_type(p);
            }
            if p.expect("{") {
                while !p.at_end() && !p.at("}") {
                    let e = p.start();
                    name_token(p);
                    if p.at("[") {
                        dim(p);
                    }
                    if p.eat("=") {
                        expr(p);
                    }
                    e.complete(p, Declarator);
                    if !p.eat(",") {
                        break;
                    }
                }
                p.expect("}");
            }
        }
        "struct" | "union" => {
            p.bump();
            p.eat("tagged");
            p.eat("soft");
            p.eat("packed");
            if p.at_any(&["signed", "unsigned"]) {
                p.bump();
            }
            if p.expect("{") {
                while !p.at_end() && !p.at("}") && !p.at_any(END_KEYWORDS) {
                    let d = p.start();
                    attributes(p);
                    data_decl(p);
                    d.complete(p, DataDecl);
                }
                p.expect("}");
            }
        }
        "virtual" => {
            p.bump();
            p.eat("interface");
            name_token(p);
            if p.at("#") && p.nth(1) == "(" {
                param_values(p);
            }
            if p.eat(".") {
                name_token(p);
            }
        }
        "interface" => {
            p.bump();
            name_token(p);
            if p.eat(".") {
                name_token(p);
            }
        }
        "type" => {
            p.bump();
            p.expect("(");
            crate::expr::type_or_expr(p);
            p.expect(")");
        }
        "signed" | "unsigned" | "[" => {}
        s if BUILTIN_TYPES.contains(&s) => {
            p.bump();
        }
        // Formal argument types of properties, sequences and checkers.
        "untyped" | "sequence" | "property" => {
            p.bump();
        }
        _ => {
            if type_name_end(p, p.pos()).is_some() {
                p.bump();
                loop {
                    if p.at("#") && p.nth(1) == "(" {
                        param_values(p);
                    }
                    if p.at("::") {
                        p.bump();
                        p.bump();
                        continue;
                    }
                    break;
                }
                // A non-ANSI interface port `I.mp x;`.
                if p.at(".") && ident_at(p, p.pos() + 1) && ident_at(p, p.pos() + 2) {
                    p.bump();
                    p.bump();
                }
            } else {
                p.error(format!("expected a data type, found `{}`", p.cur()));
            }
        }
    }
    if p.at_any(&["signed", "unsigned"]) {
        p.bump();
    }
    while p.at("[") {
        dim(p);
    }
    m.complete(p, DataType);
}

/// `[ ]`, `[$]`, `[$:n]`, `[*]`, `[type]`, `[n]`, `[a:b]`, `[a+:w]`.
pub(crate) fn dim(p: &mut Parser) {
    let m = p.start();
    p.bump();
    if p.at("]") {
    } else if p.at("$") && matches!(p.nth(1), "]" | ":") {
        p.bump();
        if p.eat(":") {
            expr(p);
        }
    } else if p.at("*") && p.nth(1) == "]" {
        p.bump();
    } else {
        crate::expr::type_or_expr(p);
        if p.at_any(&[":", "+:", "-:"]) {
            p.bump();
            expr(p);
        }
    }
    p.expect("]");
    m.complete(p, Dim);
}

/// `[qualifiers] type declarator {, declarator} ;`
pub(crate) fn data_decl(p: &mut Parser) {
    let mut quals = 0;
    // `virtual` here starts a virtual interface type, not a qualifier.
    while p.at_any(DECL_QUALIFIERS) && !p.at("virtual") {
        p.bump();
        quals += 1;
    }
    if p.at_any(NET_TYPES) {
        p.bump();
        strength(p);
        if p.at_any(&["vectored", "scalared"]) {
            p.bump();
        }
        // An explicit data type, or an implicit one.
        if (!(p.at_ident() && matches!(p.nth(1), "=" | ";" | "," | "[")) || type_with_dims(p)) && !p.at("#") {
            data_type(p);
        }
        if p.at("#") {
            timing_control(p);
        }
    } else if quals > 0 && p.at_ident() && matches!(p.nth(1), "=" | ";" | ",") {
        // `var x = 1;`, `input a;`: implicit type.
    } else {
        data_type(p);
        if p.at("#") {
            timing_control(p);
        }
    }
    loop {
        declarator(p);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}

fn declarator(p: &mut Parser) {
    let m = p.start();
    name_token(p);
    while p.at("[") {
        dim(p);
    }
    if p.eat("=") {
        expr(p);
    }
    m.complete(p, Declarator);
}

/// `parameter|localparam [type] name = value {, ...} ;`, also `type` params.
pub(crate) fn param_decl(p: &mut Parser) {
    p.bump();
    if p.eat("type") {
        loop {
            let d = p.start();
            name_token(p);
            if p.eat("=") {
                data_type(p);
            }
            d.complete(p, Declarator);
            if !p.eat(",") {
                break;
            }
        }
        p.expect(";");
        return;
    }
    if !(p.at_ident() && matches!(p.nth(1), "=" | "," | ";" | "[")) || type_with_dims(p) {
        data_type(p);
    }
    loop {
        declarator(p);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}

/// `T [2:0] name`: a type name with packed dimensions, then the name
/// (not a name with unpacked ones).
pub(crate) fn type_with_dims(p: &Parser) -> bool {
    if !(p.at_ident() && p.nth(1) == "[") {
        return false;
    }
    let mut k = 1;
    let mut depth = 0;
    while k < 4096 {
        match p.nth(k) {
            "[" => depth += 1,
            "]" => {
                depth -= 1;
                if depth == 0 && p.nth(k + 1) != "[" {
                    return matches!(
                        p.nth_kind(k + 1),
                        Some(sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier)
                    );
                }
            }
            "" => return false,
            _ => {}
        }
        k += 1;
    }
    false
}

/// `(strong0, weak1)` or `(pull1)` drive strength, if present.
fn strength(p: &mut Parser) {
    if p.at("(") && STRENGTHS.contains(&p.nth(1)) {
        group(p);
    }
}

/// `type [#(params)] name [dims] (ports) {, name (ports)} ;`
fn instantiation(p: &mut Parser) {
    let t = p.start();
    p.bump();
    while p.at("::") {
        p.bump();
        name_token(p);
    }
    if p.at("#") && p.nth(1) == "(" {
        param_values(p);
    } else if p.at("#") {
        timing_control(p);
    }
    t.complete(p, DataType);
    loop {
        let i = p.start();
        name_token(p);
        while p.at("[") {
            dim(p);
        }
        if p.at("(") {
            arg_list(p);
        } else {
            p.expect("(");
        }
        i.complete(p, Instance);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}

/// `and #1 g1 (o, a, b), (o2, c, d);`
fn gate_instantiation(p: &mut Parser) {
    p.bump();
    strength(p);
    if p.at("#") {
        timing_control(p);
    }
    loop {
        let i = p.start();
        if p.at_ident() {
            name_token(p);
            while p.at("[") {
                dim(p);
            }
        }
        if p.at("(") {
            arg_list(p);
        } else {
            p.expect("(");
        }
        i.complete(p, Instance);
        if !p.eat(",") {
            break;
        }
    }
    p.expect(";");
}

/// `( expr )` around a condition.
pub(crate) fn paren_expr(p: &mut Parser) {
    p.expect("(");
    expr_with(p, ASSIGNABLE);
    p.expect(")");
}

// ---- statements -------------------------------------------------------

/// Parses one statement (or local declaration). False if none starts here.
pub(crate) fn statement(p: &mut Parser) -> bool {
    if p.at_end() || p.at_any(END_KEYWORDS) {
        return false;
    }
    let m = p.start();
    attributes(p);
    // `label :` (not `::`)
    if p.at_ident() && p.nth(1) == ":" && p.nth(2) != ":" {
        let l = p.start();
        p.bump();
        p.bump();
        l.complete(p, StmtLabel);
    }
    let kind = statement_body(p);
    m.complete(p, kind);
    true
}

fn statement_or_missing(p: &mut Parser) {
    if !statement(p) {
        p.error("expected a statement".into());
    }
}

fn else_clause(p: &mut Parser) {
    if p.at("else") {
        let e = p.start();
        p.bump();
        statement_or_missing(p);
        e.complete(p, ElseClause);
    }
}

fn statement_body(p: &mut Parser) -> SyntaxKind {
    match p.cur() {
        ";" => {
            p.bump();
            EmptyStmt
        }
        "begin" => {
            p.bump();
            end_label(p);
            while !p.at_end() && !p.at("end") && !at_foreign_end(p, "end") {
                if !statement(p) {
                    stray(p);
                }
            }
            p.expect("end");
            end_label(p);
            SeqBlock
        }
        "fork" => {
            p.bump();
            end_label(p);
            while !p.at_end() && !p.at_any(&["join", "join_any", "join_none"]) && !at_foreign_end(p, "join") {
                if !statement(p) {
                    stray(p);
                }
            }
            if p.at_any(&["join", "join_any", "join_none"]) {
                p.bump();
            } else {
                p.error("expected `join`".into());
            }
            end_label(p);
            ParBlock
        }
        "unique" | "unique0" | "priority" => {
            p.bump();
            statement_body(p)
        }
        "if" => {
            p.bump();
            paren_expr(p);
            statement_or_missing(p);
            else_clause(p);
            IfStmt
        }
        "case" | "casex" | "casez" => {
            p.bump();
            paren_expr(p);
            let matches = p.at("matches");
            if p.at_any(&["inside", "matches"]) {
                p.bump();
            }
            case_items(p, matches);
            p.expect("endcase");
            CaseStmt
        }
        "randcase" => {
            p.bump();
            case_items(p, false);
            p.expect("endcase");
            CaseStmt
        }
        "for" => {
            p.bump();
            p.expect("(");
            if !p.at(";") {
                // After one declaration, the rest are declarations too,
                // with the type optional: `int i = 0, j = n`.
                let mut decls = false;
                loop {
                    if is_decl_start(p) || (decls && p.at_ident() && p.nth(1) == "=") {
                        let d = p.start();
                        p.eat("var");
                        if is_decl_start(p) {
                            data_type(p);
                        }
                        declarator(p);
                        d.complete(p, DataDecl);
                        decls = true;
                    } else {
                        expr_with(p, ASSIGNABLE);
                    }
                    if !p.eat(",") {
                        break;
                    }
                }
            }
            p.expect(";");
            if !p.at(";") {
                expr(p);
            }
            p.expect(";");
            if !p.at(")") {
                loop {
                    expr_with(p, ASSIGNABLE);
                    if !p.eat(",") {
                        break;
                    }
                }
            }
            p.expect(")");
            statement_or_missing(p);
            LoopStmt
        }
        "foreach" => {
            p.bump();
            group(p);
            // IEEE 1800 12.7.3: a foreach body is a statement, not a null one.
            if p.at(";") {
                p.error("expected statement".into());
            }
            statement_or_missing(p);
            LoopStmt
        }
        "while" | "repeat" => {
            p.bump();
            paren_expr(p);
            statement_or_missing(p);
            LoopStmt
        }
        "forever" => {
            p.bump();
            statement_or_missing(p);
            LoopStmt
        }
        "do" => {
            p.bump();
            statement_or_missing(p);
            p.expect("while");
            paren_expr(p);
            p.expect(";");
            DoWhileStmt
        }
        "#" | "##" | "@" | "@@" => {
            timing_control(p);
            statement_or_missing(p);
            TimedStmt
        }
        "wait" if p.nth(1) == "fork" => {
            to_semi(p);
            WaitStmt
        }
        "wait" => {
            p.bump();
            paren_expr(p);
            statement_or_missing(p);
            WaitStmt
        }
        "wait_order" => {
            p.bump();
            group(p);
            if !p.at("else") {
                statement_or_missing(p);
            }
            else_clause(p);
            WaitStmt
        }
        "assert" | "assume" | "cover" | "restrict" | "expect"
            if p.cur() == "expect" || matches!(p.nth(1), "property" | "sequence") =>
        {
            crate::assertions::concurrent_assertion(p);
            ConcurrentAssert
        }
        "assert" | "assume" | "cover" => {
            p.bump();
            if p.at("#") {
                p.bump();
                p.bump();
            } else {
                p.eat("final");
            }
            paren_expr(p);
            // action_block ::= statement_or_null | [statement] else statement_or_null
            // (IEEE 1800-2023 12.4, 16.3): a null pass statement (`;`) ends the
            // assertion, so in `if (c) assert (a); else ...` the `else` is the if's.
            if p.at(";") {
                p.bump();
            } else {
                if !p.at("else") {
                    statement_or_missing(p);
                }
                else_clause(p);
            }
            AssertStmt
        }
        "return" => {
            p.bump();
            if !p.at(";") {
                expr(p);
            }
            p.expect(";");
            ReturnStmt
        }
        "break" | "continue" => {
            p.bump();
            p.expect(";");
            JumpStmt
        }
        "disable" => {
            to_semi(p);
            DisableStmt
        }
        "->" | "->>" => {
            p.bump();
            if p.at_any(&["#", "@", "##"]) {
                timing_control(p);
            }
            expr_bp(p, 30, Default::default());
            p.expect(";");
            EventTrigger
        }
        "assign" | "force" => {
            p.bump();
            expr_with(p, ASSIGNABLE);
            p.expect(";");
            ProcAssign
        }
        "deassign" | "release" => {
            p.bump();
            expr(p);
            p.expect(";");
            ProcAssign
        }
        "randsequence" => crate::misc::randsequence(p),
        "typedef" => typedef(p),
        "parameter" | "localparam" => {
            param_decl(p);
            ParamDecl
        }
        "import" => {
            to_semi(p);
            ImportDecl
        }
        "let" => {
            crate::assertions::let_decl(p);
            LetDecl
        }
        "nettype" => crate::misc::nettype(p),
        _ => {
            // A checker instance in procedural code.
            if is_instantiation(p) {
                instantiation(p);
                return Instantiation;
            }
            if is_decl_start(p) {
                data_decl(p);
                return DataDecl;
            }
            if expr_with(p, STATEMENT) {
                p.expect(";");
            } else {
                to_semi(p);
            }
            ExprStmt
        }
    }
}

/// Case items: `expr {, expr} : stmt` or `default [:] stmt`, to `endcase`.
fn case_items(p: &mut Parser, matches: bool) {
    while !p.at_end() && !p.at("endcase") && !at_foreign_end(p, "endcase") && !p.at("end") {
        let before = p.pos();
        if p.at("default") {
            p.bump();
            p.eat(":");
        } else {
            loop {
                if matches {
                    crate::expr::pattern(p);
                    if p.eat("&&&") {
                        expr(p);
                    }
                } else {
                    value_range(p);
                }
                if !p.eat(",") {
                    break;
                }
            }
            p.expect(":");
        }
        statement_or_missing(p);
        if p.pos() == before {
            stray(p);
        }
    }
}
