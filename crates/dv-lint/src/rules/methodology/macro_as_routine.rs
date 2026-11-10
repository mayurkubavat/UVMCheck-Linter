//! SV-CNS-002 (MacroAsRoutine): Flags multi-statement code macros that use
//! nothing only a macro can do: no token pasting, stringification, nested
//! macros, `__FILE__, type arguments, declarations or return/break.

use crate::support::defines_packages::{define_src, is_overridable_default, origin_loc, user_defines};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::lexer::Kind;

const DECLARATION_KEYWORDS: &[&str] = &[
    "class",
    "function",
    "task",
    "typedef",
    "covergroup",
    "constraint",
    "module",
    "interface",
    "property",
    "sequence",
    "rand",
    "randc",
    "local",
    "protected",
    "static",
    "virtual",
    "assert",
    "cover",
    "import",
    "always",
    "always_comb",
    "always_ff",
    "always_latch",
    "initial",
    "final",
    "assign",
    "localparam",
    "parameter",
    "generate",
];

pub(crate) fn macro_as_routine(_: &Design, csts: &[Cst], out: &mut Sink) {
    for d in user_defines(csts) {
        if is_overridable_default(define_src(csts, &d), &d) {
            continue;
        }
        let formals: HashSet<&str> = d.formals.iter().flatten().map(|s| s.as_str()).collect();
        let (mut semis, mut depth) = (0, 0i32);
        let (mut has_begin, mut macro_only) = (false, false);
        for (i, (k, t)) in d.body.iter().enumerate() {
            if macro_only {
                break;
            }
            let t = t.as_str();
            match k {
                Kind::Directive | Kind::MacroPaste | Kind::MacroQuote | Kind::MacroEscapedQuote => macro_only = true,
                Kind::Keyword if DECLARATION_KEYWORDS.contains(&t) => macro_only = true,
                Kind::Keyword if matches!(t, "return" | "break" | "continue" | "disable") => macro_only = true,
                _ => {}
            }
            if *k == Kind::Identifier
                && formals.contains(t)
                && d.body.get(i + 1).is_some_and(|(_, n)| n == "::" || n == "#")
            {
                macro_only = true; // a type argument
            }
            match t {
                "(" | "{" => depth += 1,
                ")" | "}" => depth -= 1,
                ";" if depth == 0 => semis += 1,
                _ => {}
            }
            if *k == Kind::Keyword && (t == "begin" || t == "fork") {
                has_begin = true;
            }
        }
        if macro_only || !(semis >= 2 || (has_begin && semis >= 1)) {
            continue;
        }
        let kind = if formals.is_empty() { "function or task" } else { "function or task with arguments" };
        out.add(
            "SV-CNS-002",
            "MacroAsRoutine",
            Severity::Info,
            format!(
                "`define {} is a block of statements that uses nothing only a macro can do. Write it as a {kind}: arguments are \
type-checked, it can be stepped through in a debugger, and errors point at its own lines instead of every expansion.",
                d.name
            ),
            origin_loc(&csts[d.tree], d.file, d.name_at),
        );
    }
}
