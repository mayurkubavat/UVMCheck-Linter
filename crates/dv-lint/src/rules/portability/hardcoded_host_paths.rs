//! PORT-PTH-001 (HardcodedHostPaths): Disallow hardcoded absolute filesystem
//! paths (/home/..., C:\...) in file operations ($readmemh, $fopen).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use crate::support::ast::{Ast, call_site, string_value, subtree, unparen};

fn hardcoded_paths(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in subtree(c, c.tree.root) {
        let Some(call) = call_site(c, n) else { continue };
        let name = call.name.strip_prefix('$').unwrap_or(&call.name);
        if !matches!(name, "readmemh" | "readmemb" | "writememh" | "writememb" | "fopen") {
            continue;
        }
        let Some(Some(arg)) = call.args.first().copied() else { continue };
        let arg = unparen(c, arg);
        let Some(path) = string_value(c, arg) else { continue };
        let b = path.as_bytes();
        let unix = path.starts_with('/');
        let win = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
        if !(unix || win) {
            continue;
        }
        let Some(at) = c.first_tok(arg) else { continue };
        s.add(
            "PORT-PTH-001",
            "HardcodedHostPaths",
            Severity::Error,
            format!(
                "Hardcoded absolute host filesystem path '{path}' in system task '${name}'. \
                 Absolute paths break regression portability across Linux CI/CD containers and regression clouds. \
                 Use relative paths or parameterize the path via '$value$plusargs(\"path=%s\", ...)'."
            ),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

pub(crate) fn ast_hardcoded_paths(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        hardcoded_paths(c, &Ast::new(d, c), s);
    }
}
