//! Grammar, slice 1: design units, classes, packages and their items, and
//! statements at block level. Expressions, types, port lists and
//! constraint bodies are kept as balanced token runs inside their nodes;
//! later slices parse them in place.

use crate::parser::Parser;
use crate::SyntaxKind::{self, *};

/// Keywords that close a scope. A statement or item never consumes one it
/// does not own, which keeps one bad construct from swallowing the rest.
const END_KEYWORDS: &[&str] = &[
    "end", "join", "join_any", "join_none", "endcase", "endfunction", "endtask", "endclass", "endmodule",
    "endinterface", "endprogram", "endpackage", "endchecker", "endgenerate", "endgroup", "endproperty",
    "endsequence", "endclocking", "endspecify", "endconfig", "endprimitive", "endtable",
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

fn attributes(p: &mut Parser) {
    while p.at("(") && p.nth(1) == "*" && p.joined(1) {
        let m = p.start();
        p.bump();
        p.bump();
        while !p.at_end() && !(p.at("*") && p.nth(1) == ")" && p.joined(1)) {
            p.bump();
        }
        p.bump();
        p.bump();
        m.complete(p, Attribute);
    }
}

/// Consumes one balanced group starting at an opening bracket.
fn group(p: &mut Parser) {
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
fn to_semi(p: &mut Parser) {
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
fn to_end_keyword(p: &mut Parser, end_kw: &str) {
    while !p.at_end() && !p.at(end_kw) {
        p.bump();
    }
    p.expect(end_kw);
    end_label(p);
}

fn end_label(p: &mut Parser) {
    if p.at(":") && p.nth(1) != ":" {
        let m = p.start();
        p.bump();
        p.bump();
        m.complete(p, EndLabel);
    }
}

fn name_token(p: &mut Parser) {
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
    "extern", "pure", "virtual", "static", "protected", "local", "rand", "randc", "const", "automatic", "var",
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
        if p.at("extern") && matches!(head, "module" | "macromodule" | "interface" | "program") {
            to_semi(p);
            return Some(OtherItem);
        }
        if ctx == Ctx::Top && p.at("virtual") && head == "interface" && p.nth(q + 1) == "class" {
            return Some(class_decl(p, q + 1));
        }
        to_semi(p);
        return Some(if (0..q).any(|i| p.nth(i) == "const") { DataDecl } else { DataDecl });
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
            to_semi(p);
            DpiImport
        }
        "export" if p.nth_kind(1) == Some(sv_lexer::Kind::StringLiteral) => {
            to_semi(p);
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
        "parameter" | "localparam" | "specparam" | "defparam" => {
            to_semi(p);
            ParamDecl
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
            to_semi(p);
            ModportDecl
        }
        "bind" => {
            to_semi(p);
            BindDirective
        }
        "let" => {
            to_semi(p);
            LetDecl
        }
        "assign" => {
            to_semi(p);
            ContinuousAssign
        }
        "covergroup" => {
            p.bump();
            name_token(p);
            to_end_keyword(p, "endgroup");
            CovergroupDecl
        }
        "property" => {
            p.bump();
            name_token(p);
            to_end_keyword(p, "endproperty");
            PropertyDecl
        }
        "sequence" => {
            p.bump();
            name_token(p);
            to_end_keyword(p, "endsequence");
            SequenceDecl
        }
        "clocking" => clocking(p),
        "default" | "global" if p.nth(1) == "clocking" => clocking(p),
        "default" if p.nth(1) == "disable" => {
            to_semi(p);
            OtherItem
        }
        "config" => other_block(p, "endconfig"),
        "primitive" => other_block(p, "endprimitive"),
        "specify" => other_block(p, "endspecify"),
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
        "assert" | "assume" | "cover" | "restrict" | "expect" => {
            statement(p);
            AssertStmt
        }
        "for" if ctx != Ctx::Class => {
            p.bump();
            group(p);
            generate_block(p);
            LoopGenerate
        }
        "if" if ctx != Ctx::Class => {
            p.bump();
            group(p);
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
            group(p);
            while !p.at_end() && !p.at("endcase") && !p.at_any(&END_KEYWORDS[5..]) {
                if p.at("begin") {
                    generate_block(p);
                } else {
                    p.bump();
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
                statement(p);
                return Some(AssertStmt);
            }
            to_semi(p);
            DataDecl
        }
    };
    Some(k)
}

/// True at an end keyword of an enclosing construct other than `own`.
fn at_foreign_end(p: &Parser, own: &str) -> bool {
    p.at_any(END_KEYWORDS) && !p.at(own) && !matches!(p.cur(), "end" | "endcase")
}

fn generate_block(p: &mut Parser) {
    if p.at("begin") {
        let m = p.start();
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

fn other_block(p: &mut Parser, end_kw: &str) -> SyntaxKind {
    to_end_keyword(p, end_kw);
    OtherBlock
}

fn clocking(p: &mut Parser) -> SyntaxKind {
    // `default clocking name;` refers to a block; one with an event is a block.
    let start = p.pos();
    let mut i = start;
    let mut has_event = false;
    while i < start + 64 {
        match p.text_at(i) {
            ";" | "" => break,
            "@" => has_event = true,
            _ => {}
        }
        i += 1;
    }
    if has_event {
        to_end_keyword(p, "endclocking");
    } else {
        to_semi(p);
    }
    ClockingDecl
}

/// module / interface / program / checker / package.
fn unit(p: &mut Parser, end_kw: &str, kind: SyntaxKind) -> SyntaxKind {
    p.bump();
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    name_token(p);
    let h = p.start();
    to_semi(p);
    h.complete(p, Header);
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
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    name_token(p);
    let h = p.start();
    to_semi(p);
    h.complete(p, Header);
    while !p.at_end() && !p.at("endclass") && !at_foreign_end(p, "endclass") {
        if !item(p, Ctx::Class) {
            stray(p);
        }
    }
    p.expect("endclass");
    end_label(p);
    ClassDecl
}

/// Finds the method name in a header `[lifetime] [type] name (` or `;`:
/// the trailing `id (:: id)*` chain, where a scope may carry `#( ... )`.
/// Returns (name start, name end) as absolute token indices.
fn find_method_name(p: &Parser) -> Option<(usize, usize)> {
    let mut i = p.pos();
    let limit = i + 400;
    loop {
        match p.text_at(i) {
            "(" | ";" | "" => break,
            "#" if p.text_at(i + 1) == "(" => i = skip_group_at(p, i + 1),
            "[" => i = skip_group_at(p, i),
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
    // Walk back over `scope ::` pairs.
    while s >= p.pos() + 2 && p.text_at(s - 1) == "::" {
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
fn subroutine(p: &mut Parser, q: usize, proto: bool) -> SyntaxKind {
    for _ in 0..q {
        p.bump();
    }
    let is_task = p.at("task");
    p.bump();
    if p.at_any(&["static", "automatic"]) {
        p.bump();
    }
    match find_method_name(p) {
        Some((s, e)) => {
            if s > p.pos() {
                let r = p.start();
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
        let l = p.start();
        group(p);
        l.complete(p, ParenList);
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
    let ty = p.start();
    while p.pos() < n {
        if p.at_any(&["(", "[", "{", "'{"]) {
            group(p);
        } else {
            p.bump();
        }
    }
    ty.complete(p, Header);
    name_token(p);
    to_semi(p);
    TypedefDecl
}

/// `[static] constraint name { ... }` or a prototype `constraint name;`.
fn constraint(p: &mut Parser, q: usize) -> SyntaxKind {
    for _ in 0..q {
        p.bump();
    }
    p.bump();
    let n = p.start();
    while !p.at_end() && !p.at_any(&["{", ";"]) {
        p.bump();
    }
    n.complete(p, Name);
    if p.at("{") {
        let b = p.start();
        group(p);
        b.complete(p, ConstraintBlock);
        ConstraintDecl
    } else {
        p.expect(";");
        ConstraintPrototype
    }
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
    if (p.at_ident()) && p.nth(1) == ":" && p.nth(2) != ":" {
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
            group(p);
            statement_or_missing(p);
            if p.at("else") {
                let e = p.start();
                p.bump();
                statement_or_missing(p);
                e.complete(p, ElseClause);
            }
            IfStmt
        }
        "case" | "casex" | "casez" | "randcase" => {
            let open = p.cur() != "randcase";
            p.bump();
            if open {
                group(p);
                if p.at_any(&["inside", "matches"]) {
                    p.bump();
                }
            }
            case_items(p);
            p.expect("endcase");
            CaseStmt
        }
        "for" | "foreach" | "while" | "repeat" => {
            p.bump();
            group(p);
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
            group(p);
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
        "wait" | "wait_order" => {
            p.bump();
            group(p);
            if p.at("else") {
                p.bump();
            }
            statement_or_missing(p);
            if p.at("else") {
                let e = p.start();
                p.bump();
                statement_or_missing(p);
                e.complete(p, ElseClause);
            }
            WaitStmt
        }
        "assert" | "assume" | "cover" | "restrict" | "expect" => {
            p.bump();
            while p.at_any(&["property", "sequence", "final"]) {
                p.bump();
            }
            if p.at("#") {
                p.bump();
                p.bump();
            }
            group(p);
            // Action block: a statement, an else, or both.
            if p.at("else") {
                let e = p.start();
                p.bump();
                statement_or_missing(p);
                e.complete(p, ElseClause);
            } else {
                statement_or_missing(p);
                if p.at("else") {
                    let e = p.start();
                    p.bump();
                    statement_or_missing(p);
                    e.complete(p, ElseClause);
                }
            }
            AssertStmt
        }
        "randsequence" => {
            to_end_keyword(p, "endsequence");
            RandSequenceStmt
        }
        "typedef" => typedef(p),
        _ => {
            to_semi(p);
            SimpleStmt
        }
    }
}

fn timing_control(p: &mut Parser) {
    // `#5`, `#(d)`, `#1ns`, `##[1:3]`, `@(posedge clk)`, `@e`, `@*`, `@ (*)`.
    let op = p.cur();
    p.bump();
    if p.at_any(&["(", "["]) {
        group(p);
    } else if op == "@" && p.at("*") {
        p.bump();
    } else {
        // A name, possibly hierarchical, or a literal.
        p.bump();
        while p.at_any(&[".", "::"]) {
            p.bump();
            p.bump();
        }
    }
}

/// Case items: `expr {, expr} : stmt` or `default [:] stmt`, to `endcase`.
fn case_items(p: &mut Parser) {
    while !p.at_end() && !p.at("endcase") && !at_foreign_end(p, "endcase") && !p.at("end") {
        if p.at("default") {
            p.bump();
            p.eat(":");
        } else {
            // Expressions up to the `:` that is not inside brackets or `?:`.
            let mut ternary = 0;
            while !p.at_end() && !p.at_any(END_KEYWORDS) {
                if p.at_any(&["(", "[", "{", "'{"]) {
                    group(p);
                    continue;
                }
                if p.at("?") {
                    ternary += 1;
                } else if p.at(":") {
                    if ternary == 0 {
                        break;
                    }
                    ternary -= 1;
                }
                p.bump();
            }
            p.expect(":");
        }
        statement_or_missing(p);
    }
}
