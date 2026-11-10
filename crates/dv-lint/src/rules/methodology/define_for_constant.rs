//! SV-CNS-001 (DefineForConstant): Flags `define macros whose body is only a
//! number or a string, used as constants.

use crate::support::defines_packages::{define_src, is_overridable_default, origin_loc, user_defines};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::lexer::Kind;

pub(crate) fn define_for_constant(_: &Design, csts: &[Cst], out: &mut Sink) {
    for d in user_defines(csts) {
        if d.formals.is_some() || d.body.is_empty() || is_overridable_default(define_src(csts, &d), &d) {
            continue;
        }
        let (mut numbers, mut strings) = (0, 0);
        let mut constant = true;
        for (k, t) in &d.body {
            match k {
                Kind::IntegerLiteral | Kind::IntegerBase | Kind::RealLiteral | Kind::TimeLiteral => numbers += 1,
                Kind::StringLiteral | Kind::TripleQuotedString => strings += 1,
                // An unterminated string is still a string literal to slang.
                Kind::Unknown if t.starts_with('"') => strings += 1,
                // slang lexes the digits after a base as an identifier, or
                // as a number and then an identifier when they start with a
                // digit and go on with letters.
                Kind::VectorDigits => {
                    if t.starts_with(|c: char| c.is_ascii_digit())
                        && t.contains(|c: char| c.is_ascii_alphabetic() || c == '?')
                    {
                        constant = false;
                    } else {
                        numbers += 1;
                    }
                }
                Kind::Punct if matches!(t.as_str(), "(" | ")" | "-" | "+" | "*" | "/" | "<<") => {}
                _ => constant = false,
            }
        }
        if !constant || (numbers == 0) == (strings == 0) || strings > 1 {
            continue;
        }
        let body: String = d.body.iter().map(|(_, t)| t.as_str()).collect();
        out.add(
            "SV-CNS-001",
            "DefineForConstant",
            Severity::Warning,
            format!(
                "`define {} holds the constant '{body}'. As a macro it has no type, no scope and can be redefined by any later file. \
Declare it as a parameter or localparam in a package instead.",
                d.name
            ),
            origin_loc(&csts[d.tree], d.file, d.name_at),
        );
    }
}
