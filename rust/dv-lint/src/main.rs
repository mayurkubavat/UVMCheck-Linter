//! Runs the ported rules over a design, built natively (default) or through
//! slang (`--slang`, model rules only), and prints findings as uvmcheck does:
//!
//!     dv-lint [--slang] [--time] [--history] [--rule ID]... [--list] <compile options>
//!
//! Exit status: 0 clean, 1 findings, 2 load error.
use dv_model::Design;

fn main() {
    let mut slang = false;
    let mut time = false;
    let mut specs = false;
    let mut exprs = false;
    let mut only: Vec<String> = Vec::new();
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--slang" => slang = true,
            "--time" => time = true,
            "--specs" => specs = true,
            // Each expression's type, as tools/exprdump.cpp prints slang's.
            "--exprs" => exprs = true,
            "--rule" => only.extend(it.next()),
            // Read git history for SV-EVO-001 and SV-EVO-002.
            "--history" => dv_lint::set_history(true),
            // The design's tops, as slang's --top: modules no top reaches
            // are not elaborated.
            "-top" | "--top" => {
                let t: Vec<String> = it.next().into_iter().collect();
                dv_lint::add_tops(&t);
            }
            "--list" => {
                for r in dv_lint::rules() {
                    let kind = if r.is_syntax() { "syntax" } else { "model" };
                    println!("{} {} {}", r.id, r.name, kind);
                    for (id, name) in dv_lint::sub_rules(r.id) {
                        println!("{id} {name} {kind}");
                    }
                }
                return;
            }
            _ => args.push(a),
        }
    }
    let loaded = if slang {
        load_slang(&args).map(|d| (d, Vec::new()))
    } else {
        sv_sema::load_with_trees(&args).map_err(|e| e.to_string())
    };
    let (design, trees) = match loaded {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    if exprs {
        for l in dv_lint::g_exprty::expr_dump(design.files().to_vec(), &trees) {
            println!("{l}");
        }
        return;
    }
    if specs {
        // How often the AST walk visits each generic class (`name@file:line`).
        for l in dv_lint::spec_report(&design, &trees) {
            println!("{l}");
        }
        return;
    }
    let t = std::time::Instant::now();
    let (files, found) = dv_lint::lint_all(&design, &trees, |id| {
        only.is_empty() || only.iter().any(|o| o == id)
    });
    if time {
        let tm = design.timings();
        eprintln!(
            "parse {:.0} ms, elaborate {:.0} ms, model {:.0} ms, copy {:.0} ms, rules {:.0} ms",
            tm.parse_ms,
            tm.elaborate_ms,
            tm.model_ms,
            tm.copy_ms,
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    for f in &found {
        let (file, line, column) = f.position_in(&files);
        println!(
            "{}:{}:{}: [{}] {} ({}): {}",
            file,
            line,
            column,
            f.severity.as_str(),
            f.rule,
            f.name,
            f.message
        );
        for (loc, note) in &f.related {
            println!(
                "    note: {}:{}:{}: {}",
                files.get(loc.file as usize).map(String::as_str).unwrap_or(""),
                loc.line,
                loc.column,
                note
            );
        }
    }
    std::process::exit(if found.is_empty() { 0 } else { 1 });
}

#[cfg(feature = "slang")]
fn load_slang(args: &[String]) -> Result<Design, String> {
    Design::load(args).map_err(|e| e.to_string())
}

#[cfg(not(feature = "slang"))]
fn load_slang(_: &[String]) -> Result<Design, String> {
    Err("built without the slang feature".into())
}
