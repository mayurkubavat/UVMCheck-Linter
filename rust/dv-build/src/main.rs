//! dv-build: the native SystemVerilog builder. Reads compile options and
//! VCS/Xcelium file lists, parses every source with the native front end
//! and reports what is wrong, as a compiler would. It grows phase by phase
//! (see the builder plan): today it checks preprocessing and syntax,
//! builds a scope for every design unit and checks the names declared in
//! them; types, constants, elaboration and type checking follow.
//!
//!     dv-build [--dialect any|vcs|xcelium] [--dump] [--symbols] [--types] [--values] [--hier] [--quiet] <options, e.g. -f top.f FILE...>
//!
//! Diagnostics go to stdout as `file:line:col: error: message`, then a
//! summary line `errors N`. --dump also prints the design: one line per
//! declaration in source order (`module top`, `  class pkt`), the form the
//! Slang twin prints for the gate (tools/ieee_gate.py). --symbols prints
//! the scopes (phase N1): one sorted `<scope path> <kind> <name>` line per
//! declared symbol, the form of tools/sym_dump.cpp (tools/sym_gate.py);
//! --types adds ` : <type>` to values and typedefs (phase N2), --values
//! ` = <value>` to parameters, specparams and enum values (phase N3).
//! --hier prints the elaborated design instead: instances, generate blocks
//! and parameter values by hierarchical path (phase N4).
//! Exit status: 0 no errors, 1 errors, 2 bad usage or unreadable input.

use std::process::ExitCode;

use sv_sema::Locator;
use sv_source::{Dialect, FileList, ProcessEnv, Severity};
use sv_syntax::{Options, Tree};

struct Diag {
    file: String,
    line: u32,
    column: u32,
    message: String,
}

fn main() -> ExitCode {
    let mut dialect = Dialect::Any;
    let (mut dump, mut quiet, mut symbols, mut types, mut values) = (false, false, false, false, false);
    let mut hier = false;
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dump" => dump = true,
            "--quiet" => quiet = true,
            "--symbols" => symbols = true,
            "--hier" => hier = true,
            "--values" => {
                symbols = true;
                values = true;
            }
            "--types" => {
                symbols = true;
                types = true;
            }
            "--dialect" => {
                dialect = match it.next().as_deref() {
                    Some("vcs") => Dialect::Vcs,
                    Some("xcelium") => Dialect::Xcelium,
                    Some("any") => Dialect::Any,
                    other => {
                        eprintln!("error: --dialect takes any, vcs or xcelium, not {other:?}");
                        return ExitCode::from(2);
                    }
                }
            }
            "-h" | "--help" => {
                println!(
                    "usage: dv-build [--dialect any|vcs|xcelium] [--dump] [--symbols] [--types] [--values] [--hier] [--quiet] <options> FILE..."
                );
                return ExitCode::SUCCESS;
            }
            _ => args.push(a),
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let list = FileList::read_args(&args, &cwd, dialect, &ProcessEnv);
    let mut diags = Vec::new();
    for d in &list.diagnostics {
        if d.severity == Severity::Error {
            diags.push(Diag {
                file: d.origin.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                line: d.origin.line,
                column: 0,
                message: d.message.clone(),
            });
        }
    }
    if list.sources.is_empty() {
        eprintln!("error: no sources given");
        return ExitCode::from(2);
    }
    let opts = Options {
        include_dirs: list.include_dirs.iter().map(|d| d.value.clone()).collect(),
        defines: list
            .effective_defines()
            .into_iter()
            .map(|(n, v)| match v {
                Some(v) => format!("{n}={v}"),
                None => n,
            })
            .collect(),
        ..Options::default()
    };
    let mut trees = Vec::new();
    for s in &list.sources {
        if !s.value.exists() {
            eprintln!("error: cannot read {}", s.value.display());
            return ExitCode::from(2);
        }
        trees.push(sv_syntax::parse_file(&s.value, opts.clone()));
    }
    let mut loc = Locator::default();
    for (ix, t) in trees.iter().enumerate() {
        syntax_diagnostics(ix, t, &mut loc, &mut diags);
    }
    let scopes = sv_sema::scopes::build(&trees);
    let mut checks = sv_sema::names::check(&trees, &scopes);
    checks.extend(sv_sema::checks::check(&trees, &scopes));
    let (hier_lines, elab_diags) = sv_sema::elaborate(&trees);
    checks.extend(elab_diags);
    checks.extend(sv_sema::bind::check(&trees));
    checks.extend(sv_sema::iclass::check(&trees));
    for d in checks {
        let t = &trees[d.tree];
        let Some((tok, _)) = t.token_bounds(d.node) else {
            continue;
        };
        let l = loc.loc(d.tree, t, tok);
        let (file, line, column) = match l.expansion {
            Some((f, line)) => (f, line, 0),
            None => (l.file, l.line, l.column),
        };
        let file = loc.files().get(file as usize).cloned().unwrap_or_default();
        diags.push(Diag { file, line, column, message: d.message });
    }
    if !quiet {
        for d in &diags {
            println!("{}:{}:{}: error: {}", d.file, d.line, d.column, d.message);
        }
    }
    if dump {
        for t in &trees {
            print_dump(t);
        }
    }
    if hier {
        for l in hier_lines {
            println!("{l}");
        }
    } else if symbols {
        let (tys, vals) = if types || values { sv_sema::symbol_info(&trees, &scopes) } else { Default::default() };
        let lines = scopes.dump_full(types.then_some(tys.as_slice()), values.then_some(vals.as_slice()));
        for l in lines {
            println!("{l}");
        }
    }
    println!("errors {}", diags.len());
    if diags.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn syntax_diagnostics(ix: usize, t: &Tree, loc: &mut Locator, out: &mut Vec<Diag>) {
    let files = |loc: &Locator, f: u32| loc.files().get(f as usize).cloned().unwrap_or_default();
    for d in &t.pp_diagnostics {
        let (f, line, column) = loc.position(ix, t, d.origin);
        out.push(Diag { file: files(loc, f), line, column, message: d.message.clone() });
    }
    for e in &t.errors {
        // An error at the end of input sits after the last token.
        let tok = (e.token as usize).min(t.tokens.len().saturating_sub(1)) as u32;
        if t.tokens.is_empty() {
            out.push(Diag { file: String::new(), line: 0, column: 0, message: e.message.clone() });
            continue;
        }
        let l = loc.loc(ix, t, tok);
        let (file, line, column) = match l.expansion {
            Some((f, line)) => (f, line, 0),
            None => (l.file, l.line, l.column),
        };
        out.push(Diag { file: files(loc, file), line, column, message: e.message.clone() });
    }
}

/// Declarations in source order, indented by nesting.
fn print_dump(t: &Tree) {
    let entries = sv_syntax::outline(t);
    let mut depth_of = std::collections::HashMap::new();
    let mut parent = vec![u32::MAX; t.nodes.len()];
    for n in t.preorder() {
        for c in t.children(n) {
            parent[c.0 as usize] = n.0;
        }
    }
    for e in &entries {
        let mut d = 0usize;
        let mut p = parent[e.node.0 as usize];
        while p != u32::MAX {
            if let Some(&pd) = depth_of.get(&p) {
                d = pd + 1;
                break;
            }
            p = parent[p as usize];
        }
        depth_of.insert(e.node.0, d);
        println!("{}{} {}", "  ".repeat(d), e.kind, e.name);
    }
}
